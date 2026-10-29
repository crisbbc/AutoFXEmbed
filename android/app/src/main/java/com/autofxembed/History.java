package com.autofxembed;

import android.content.Context;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * Converted links, newest first, with the embed data (title / description)
 * read from each embed page so the Recent list can say what they are. Stored as
 * JSON in the app's files dir, in the same shape as the desktop history.
 */
final class History {
    private static final String TAG = "AutoFxEmbed";
    /** Most entries kept; the oldest are dropped first. */
    private static final int MAX_ENTRIES = 100;
    /** Entries shown on the main screen. */
    static final int SHOWN_ENTRIES = 10;
    private static final int MAX_BODY_BYTES = 512 * 1024;
    private static final int TIMEOUT_MS = 10_000;
    /** The fx* hosts only serve their embed page to link-preview bots. */
    private static final String USER_AGENT =
            "Mozilla/5.0 (compatible; Discordbot/2.0; +https://discordapp.com)";

    static final class Entry {
        String original;
        String embed;
        /** Unix time (seconds) of the latest copy. */
        long copiedAt;
        String site;
        String title;
        String description;
        String image;
        /** True once the embed data has been read. */
        boolean fetched;
    }

    private static final Object LOCK = new Object();
    private static final ExecutorService FETCHER = Executors.newSingleThreadExecutor();
    private static final Handler MAIN = new Handler(Looper.getMainLooper());
    private static List<Entry> entries;
    private static Runnable onChange;

    private History() {}

    /** Run {@code listener} on the main thread whenever the list changes; null to stop. */
    static void setOnChange(Runnable listener) {
        synchronized (LOCK) {
            onChange = listener;
        }
    }

    /** Record links that were just converted, then read their embed data. */
    static void record(Context context, List<String[]> links) {
        if (links.isEmpty()) {
            return;
        }
        long now = System.currentTimeMillis() / 1000;
        synchronized (LOCK) {
            List<Entry> list = load(context);
            // Reverse so the first link of the text ends up on top.
            for (int i = links.size() - 1; i >= 0; i--) {
                String[] link = links.get(i);
                Entry entry = null;
                for (int j = 0; j < list.size(); j++) {
                    if (list.get(j).embed.equals(link[1])) {
                        entry = list.remove(j);
                        break;
                    }
                }
                if (entry == null) {
                    entry = new Entry();
                    entry.embed = link[1];
                }
                entry.original = link[0];
                entry.copiedAt = now;
                list.add(0, entry);
            }
            while (list.size() > MAX_ENTRIES) {
                list.remove(list.size() - 1);
            }
            save(context);
        }
        changed();
        fetchPending(context);
    }

    /** The newest {@code n} entries. */
    static List<Entry> recent(Context context, int n) {
        synchronized (LOCK) {
            List<Entry> list = load(context);
            return new ArrayList<>(list.subList(0, Math.min(n, list.size())));
        }
    }

    static void clear(Context context) {
        synchronized (LOCK) {
            load(context).clear();
            save(context);
        }
        changed();
    }

    /** Read the embed data of every entry that doesn't have it yet. */
    static void fetchPending(Context context) {
        Context app = context.getApplicationContext();
        List<String> pending = new ArrayList<>();
        synchronized (LOCK) {
            for (Entry entry : load(app)) {
                if (!entry.fetched) {
                    pending.add(entry.embed);
                }
            }
        }
        for (String embed : pending) {
            FETCHER.execute(() -> fetch(app, embed));
        }
    }

    private static void fetch(Context context, String embed) {
        String[] og;
        try {
            og = readEmbed(embed);
        } catch (IOException e) {
            // Offline or a flaky server: keep the entry and try again next time.
            Log.w(TAG, "embed read failed: " + e);
            return;
        }
        synchronized (LOCK) {
            List<Entry> list = load(context);
            Entry entry = null;
            for (Entry candidate : list) {
                if (candidate.embed.equals(embed)) {
                    entry = candidate;
                }
            }
            // Cleared or trimmed while the read ran, or already done.
            if (entry == null || entry.fetched) {
                return;
            }
            if (og == null) {
                // Not an embed page (e.g. a bare host bouncing elsewhere): drop it.
                list.remove(entry);
            } else {
                entry.fetched = true;
                entry.site = og[0];
                entry.title = og[1];
                entry.description = og[2];
                entry.image = og[3];
            }
            save(context);
        }
        changed();
    }

    /** The page's {@code [site, title, description, image]}, or null if it is no embed. */
    private static String[] readEmbed(String embed) throws IOException {
        HttpURLConnection connection = (HttpURLConnection) new URL(embed).openConnection();
        try {
            connection.setConnectTimeout(TIMEOUT_MS);
            connection.setReadTimeout(TIMEOUT_MS);
            connection.setRequestProperty("User-Agent", USER_AGENT);
            if (connection.getResponseCode() >= 400) {
                return null;
            }
            String html;
            try (InputStream in = connection.getInputStream()) {
                html = new String(readUpTo(in, MAX_BODY_BYTES), StandardCharsets.UTF_8);
            }
            return Native.parseEmbed(embed, connection.getURL().toString(), html);
        } finally {
            connection.disconnect();
        }
    }

    static byte[] readUpTo(InputStream in, int max) throws IOException {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        byte[] buffer = new byte[8192];
        int read;
        while (out.size() < max) {
            read = in.read(buffer, 0, Math.min(buffer.length, max - out.size()));
            if (read == -1) {
                break;
            }
            out.write(buffer, 0, read);
        }
        return out.toByteArray();
    }

    private static void changed() {
        MAIN.post(() -> {
            Runnable listener;
            synchronized (LOCK) {
                listener = onChange;
            }
            if (listener != null) {
                listener.run();
            }
        });
    }

    // ---- persistence (call with LOCK held) ----

    private static File file(Context context) {
        return new File(context.getFilesDir(), "history.json");
    }

    private static List<Entry> load(Context context) {
        if (entries != null) {
            return entries;
        }
        entries = new ArrayList<>();
        File file = file(context);
        if (!file.exists()) {
            return entries;
        }
        try (InputStream in = new FileInputStream(file)) {
            byte[] bytes = readUpTo(in, Integer.MAX_VALUE);
            JSONArray array = new JSONArray(new String(bytes, StandardCharsets.UTF_8));
            for (int i = 0; i < array.length(); i++) {
                JSONObject json = array.getJSONObject(i);
                Entry entry = new Entry();
                entry.original = json.optString("original");
                entry.embed = json.getString("embed");
                entry.copiedAt = json.optLong("copied_at");
                entry.site = optString(json, "site");
                entry.title = optString(json, "title");
                entry.description = optString(json, "description");
                entry.image = optString(json, "image");
                entry.fetched = json.optBoolean("fetched");
                entries.add(entry);
            }
        } catch (IOException | JSONException e) {
            Log.w(TAG, "ignoring unreadable history: " + e);
        }
        return entries;
    }

    private static String optString(JSONObject json, String key) {
        return json.isNull(key) ? null : json.optString(key, null);
    }

    private static void save(Context context) {
        try {
            JSONArray array = new JSONArray();
            for (Entry entry : entries) {
                JSONObject json = new JSONObject();
                json.put("original", entry.original);
                json.put("embed", entry.embed);
                json.put("copied_at", entry.copiedAt);
                json.put("site", entry.site);
                json.put("title", entry.title);
                json.put("description", entry.description);
                json.put("image", entry.image);
                json.put("fetched", entry.fetched);
                array.put(json);
            }
            // Write-then-rename so a crash never leaves a half-written file.
            File file = file(context);
            File tmp = new File(file.getPath() + ".tmp");
            try (FileOutputStream out = new FileOutputStream(tmp)) {
                out.write(array.toString().getBytes(StandardCharsets.UTF_8));
                out.getFD().sync();
            }
            if (!tmp.renameTo(file)) {
                throw new IOException("rename failed");
            }
        } catch (IOException | JSONException e) {
            Log.w(TAG, "unable to save history: " + e);
        }
    }
}
