package com.autofxembed;

import android.app.Activity;
import android.app.AlertDialog;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageInstaller;
import android.net.Uri;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;
import android.widget.Toast;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.Locale;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Self-updater: checks the latest GitHub release and, on the user's say-so,
 * downloads the APK, verifies its SHA-256 against the digest GitHub publishes,
 * and hands it to the system installer. Android always asks the user to confirm
 * the install itself.
 */
final class Updater {
    private static final String TAG = "AutoFxEmbed";
    private static final String LATEST_URL =
            "https://api.github.com/repos/crisbbc/AutoFXEmbed/releases/latest";
    /** The release asset this app updates from (see .github/workflows/release.yml). */
    private static final String ASSET = "autofxembed-android.apk";
    private static final long CHECK_INTERVAL_MS = 24L * 60 * 60 * 1000;
    private static final int API_TIMEOUT_MS = 15_000;
    private static final int DOWNLOAD_TIMEOUT_MS = 180_000;
    private static final int MAX_API_BYTES = 1024 * 1024;
    private static final long MAX_ASSET_BYTES = 64L * 1024 * 1024;

    /** Only one check / download at a time. */
    private static final AtomicBoolean BUSY = new AtomicBoolean();
    private static final Handler MAIN = new Handler(Looper.getMainLooper());

    private Updater() {}

    private static final class Release {
        String tag;
        String pageUrl;
        String apkUrl;
        long apkSize;
        /** Lowercase hex SHA-256 of the APK, from the asset's digest. */
        String apkSha256;
    }

    static boolean isBusy() {
        return BUSY.get();
    }

    /** Automatic check, at most once a day while the toggle is on. */
    static void checkIfDue(Activity activity) {
        Prefs prefs = new Prefs(activity);
        if (prefs.autoUpdate()
                && System.currentTimeMillis() - prefs.lastCheck() >= CHECK_INTERVAL_MS) {
            check(activity, false, null);
        }
    }

    /**
     * Look for a newer release in the background and offer it. A manual check
     * also reports "up to date" and failures. {@code done} runs on the main
     * thread once the check (not the download) is over.
     */
    static void check(Activity activity, boolean manual, Runnable done) {
        if (!BUSY.compareAndSet(false, true)) {
            return;
        }
        Context app = activity.getApplicationContext();
        new Thread(() -> {
            Release release = null;
            try {
                release = fetchLatest();
                new Prefs(app).setLastCheck(System.currentTimeMillis());
            } catch (IOException | JSONException e) {
                Log.w(TAG, "update check failed: " + e);
            }
            Release result = release;
            MAIN.post(() -> {
                BUSY.set(false);
                if (done != null) {
                    done.run();
                }
                if (!activity.isFinishing() && !activity.isDestroyed()) {
                    offer(activity, result, manual);
                }
            });
        }).start();
    }

    private static void offer(Activity activity, Release release, boolean manual) {
        String current = currentVersion(activity);
        if (release == null) {
            if (manual) {
                toast(activity, activity.getString(R.string.update_failed_check));
            }
            return;
        }
        if (!Native.isNewer(release.tag, current)) {
            if (manual) {
                toast(activity, activity.getString(R.string.up_to_date, current));
            }
            return;
        }
        Prefs prefs = new Prefs(activity);
        if (!manual && release.tag.equals(prefs.skippedVersion())) {
            return;
        }

        AlertDialog.Builder dialog = new AlertDialog.Builder(activity)
                .setTitle(R.string.update_title)
                .setNeutralButton(R.string.update_release_page,
                        (d, w) -> openPage(activity, release.pageUrl));
        if (release.apkUrl == null || release.apkSha256 == null) {
            dialog.setMessage(activity.getString(R.string.update_manual, release.tag))
                    .setPositiveButton(android.R.string.ok, null);
        } else {
            dialog.setMessage(activity.getString(R.string.update_question, release.tag, current))
                    .setPositiveButton(R.string.update_install, (d, w) -> {
                        prefs.setSkippedVersion(null);
                        download(activity.getApplicationContext(), release);
                    })
                    .setNegativeButton(R.string.update_later, (d, w) -> {
                        if (!manual) {
                            prefs.setSkippedVersion(release.tag);
                        }
                    });
        }
        dialog.show();
    }

    private static Release fetchLatest() throws IOException, JSONException {
        HttpURLConnection connection = open(LATEST_URL, API_TIMEOUT_MS);
        connection.setRequestProperty("Accept", "application/vnd.github+json");
        JSONObject json;
        try (InputStream in = connection.getInputStream()) {
            json = new JSONObject(
                    new String(History.readUpTo(in, MAX_API_BYTES), StandardCharsets.UTF_8));
        } finally {
            connection.disconnect();
        }
        Release release = new Release();
        release.tag = json.getString("tag_name");
        release.pageUrl = json.getString("html_url");
        JSONArray assets = json.optJSONArray("assets");
        for (int i = 0; assets != null && i < assets.length(); i++) {
            JSONObject asset = assets.getJSONObject(i);
            if (!ASSET.equals(asset.optString("name"))) {
                continue;
            }
            release.apkUrl = asset.getString("browser_download_url");
            release.apkSize = asset.optLong("size");
            String digest = asset.optString("digest", "");
            if (digest.matches("sha256:[0-9a-fA-F]{64}")) {
                release.apkSha256 = digest.substring("sha256:".length()).toLowerCase(Locale.ROOT);
            }
        }
        return release;
    }

    private static void download(Context context, Release release) {
        if (!BUSY.compareAndSet(false, true)) {
            return;
        }
        toast(context, context.getString(R.string.update_downloading));
        new Thread(() -> {
            String error = null;
            try {
                install(context, fetchApk(release));
            } catch (IOException | NoSuchAlgorithmException e) {
                Log.w(TAG, "update failed: " + e);
                error = e.getMessage();
            }
            String failure = error;
            MAIN.post(() -> {
                BUSY.set(false);
                if (failure != null) {
                    toast(context, context.getString(R.string.update_failed, failure));
                }
            });
        }).start();
    }

    /** The release APK, checked against its size and SHA-256. */
    private static byte[] fetchApk(Release release) throws IOException, NoSuchAlgorithmException {
        if (release.apkSize <= 0 || release.apkSize > MAX_ASSET_BYTES) {
            throw new IOException("unexpected download size (" + release.apkSize + " bytes)");
        }
        HttpURLConnection connection = open(release.apkUrl, DOWNLOAD_TIMEOUT_MS);
        byte[] bytes;
        try (InputStream in = connection.getInputStream()) {
            bytes = History.readUpTo(in, (int) release.apkSize + 1);
        } finally {
            connection.disconnect();
        }
        if (bytes.length != release.apkSize) {
            throw new IOException("downloaded size does not match the release");
        }
        byte[] hash = MessageDigest.getInstance("SHA-256").digest(bytes);
        StringBuilder hex = new StringBuilder();
        for (byte b : hash) {
            hex.append(String.format(Locale.ROOT, "%02x", b));
        }
        if (!hex.toString().equals(release.apkSha256)) {
            throw new IOException("checksum mismatch");
        }
        return bytes;
    }

    /** Stream the APK into an install session; {@link InstallReceiver} gets the outcome. */
    private static void install(Context context, byte[] apk) throws IOException {
        PackageInstaller installer = context.getPackageManager().getPackageInstaller();
        PackageInstaller.SessionParams params =
                new PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL);
        params.setAppPackageName(context.getPackageName());
        int sessionId = installer.createSession(params);
        try (PackageInstaller.Session session = installer.openSession(sessionId)) {
            try (OutputStream out = session.openWrite("base.apk", 0, apk.length)) {
                out.write(apk);
                session.fsync(out);
            }
            int flags = PendingIntent.FLAG_UPDATE_CURRENT;
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                // The installer fills in the status extras.
                flags |= PendingIntent.FLAG_MUTABLE;
            }
            Intent result = new Intent(context, InstallReceiver.class);
            PendingIntent callback = PendingIntent.getBroadcast(context, sessionId, result, flags);
            session.commit(callback.getIntentSender());
        } catch (IOException | RuntimeException e) {
            installer.abandonSession(sessionId);
            throw e;
        }
    }

    private static HttpURLConnection open(String url, int timeoutMs) throws IOException {
        HttpURLConnection connection = (HttpURLConnection) new URL(url).openConnection();
        connection.setConnectTimeout(API_TIMEOUT_MS);
        connection.setReadTimeout(timeoutMs);
        connection.setRequestProperty("User-Agent", "AutoFxEmbed-Android");
        return connection;
    }

    static String currentVersion(Context context) {
        try {
            return context.getPackageManager()
                    .getPackageInfo(context.getPackageName(), 0).versionName;
        } catch (android.content.pm.PackageManager.NameNotFoundException e) {
            return "0.0.0";
        }
    }

    private static void openPage(Activity activity, String url) {
        activity.startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(url)));
    }

    private static void toast(Context context, String text) {
        Toast.makeText(context, text, Toast.LENGTH_LONG).show();
    }
}
