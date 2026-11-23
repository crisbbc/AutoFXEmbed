package com.autofxembed;

import android.content.Context;
import android.content.SharedPreferences;
import android.text.TextUtils;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/** Settings shared by the main screen, the share handler and the updater. */
final class Prefs {
    /** Prefix of a custom domain selection, as on the desktop. */
    static final String CUSTOM_PREFIX = "custom:";

    private static final String SELECTION = "x_target";
    private static final String CUSTOM_DOMAINS = "custom_domains";
    private static final String EXTRA_RULES = "extra_rules";
    private static final String REWRITE_X = "rewrite_x";
    private static final String REWRITE_BSKY = "rewrite_bsky";
    private static final String REWRITE_TIKTOK = "rewrite_tiktok";
    private static final String X_TRANSLATE = "x_translate";
    private static final String STRIP_TRACKERS = "strip_trackers";
    private static final String AUTO_UPDATE = "auto_update";
    private static final String LAST_CHECK = "last_update_check";
    private static final String SKIPPED = "skipped_version";

    private final SharedPreferences prefs;

    Prefs(Context context) {
        prefs = context.getSharedPreferences("settings", Context.MODE_PRIVATE);
    }

    /** Selected target: a built-in id or {@code custom:<domain>}. */
    String selection() {
        String selection = prefs.getString(SELECTION, "fixup");
        // A custom domain that was removed falls back to FixUpX.
        if (selection.startsWith(CUSTOM_PREFIX)
                && !customDomains().contains(selection.substring(CUSTOM_PREFIX.length()))) {
            return "fixup";
        }
        return selection;
    }

    void setSelection(String selection) {
        prefs.edit().putString(SELECTION, selection).apply();
    }

    /** The user's custom domains, in the order they were added. */
    List<String> customDomains() {
        String joined = prefs.getString(CUSTOM_DOMAINS, "");
        return joined.isEmpty()
                ? new ArrayList<>()
                : new ArrayList<>(Arrays.asList(joined.split("\n")));
    }

    /** Adds an already-normalized domain and selects it. */
    void addCustomDomain(String domain) {
        List<String> domains = customDomains();
        if (!domains.contains(domain)) {
            domains.add(domain);
        }
        prefs.edit()
                .putString(CUSTOM_DOMAINS, TextUtils.join("\n", domains))
                .putString(SELECTION, CUSTOM_PREFIX + domain)
                .apply();
    }

    void removeCustomDomain(String domain) {
        List<String> domains = customDomains();
        domains.remove(domain);
        prefs.edit().putString(CUSTOM_DOMAINS, TextUtils.join("\n", domains)).apply();
    }

    String extraRulesRaw() {
        return prefs.getString(EXTRA_RULES, "");
    }

    /** Generic {@code [source, target]} site rewrite rules. */
    List<String[]> extraRules() {
        String raw = extraRulesRaw();
        List<String[]> out = new ArrayList<>();
        if (raw.isEmpty()) {
            return out;
        }
        for (String line : raw.split("\n")) {
            int arrow = line.indexOf(" -> ");
            if (arrow > 0 && arrow + 4 < line.length()) {
                out.add(new String[] {line.substring(0, arrow), line.substring(arrow + 4)});
            }
        }
        return out;
    }

    void addExtraRule(String source, String target) {
        List<String[]> rules = extraRules();
        boolean updated = false;
        for (String[] pair : rules) {
            if (pair[0].equals(source)) {
                pair[1] = target;
                updated = true;
                break;
            }
        }
        if (!updated) {
            rules.add(new String[] {source, target});
        }
        saveExtraRules(rules);
    }

    void removeExtraRule(String source) {
        List<String[]> rules = extraRules();
        rules.removeIf(pair -> pair[0].equals(source));
        saveExtraRules(rules);
    }

    private void saveExtraRules(List<String[]> rules) {
        List<String> lines = new ArrayList<>(rules.size());
        for (String[] pair : rules) {
            lines.add(pair[0] + " -> " + pair[1]);
        }
        prefs.edit().putString(EXTRA_RULES, TextUtils.join("\n", lines)).apply();
    }

    boolean rewriteX() {
        return prefs.getBoolean(REWRITE_X, true);
    }

    void setRewriteX(boolean on) {
        prefs.edit().putBoolean(REWRITE_X, on).apply();
    }

    boolean rewriteBsky() {
        return prefs.getBoolean(REWRITE_BSKY, true);
    }

    void setRewriteBsky(boolean on) {
        prefs.edit().putBoolean(REWRITE_BSKY, on).apply();
    }

    boolean rewriteTiktok() {
        return prefs.getBoolean(REWRITE_TIKTOK, true);
    }

    void setRewriteTiktok(boolean on) {
        prefs.edit().putBoolean(REWRITE_TIKTOK, on).apply();
    }

    String xTranslateLang() {
        return prefs.getString(X_TRANSLATE, "");
    }

    void setXTranslateLang(String lang) {
        prefs.edit().putString(X_TRANSLATE, lang == null ? "" : lang).apply();
    }

    boolean stripTrackers() {
        return prefs.getBoolean(STRIP_TRACKERS, true);
    }

    void setStripTrackers(boolean on) {
        prefs.edit().putBoolean(STRIP_TRACKERS, on).apply();
    }

    boolean autoUpdate() {
        return prefs.getBoolean(AUTO_UPDATE, true);
    }

    void setAutoUpdate(boolean on) {
        prefs.edit().putBoolean(AUTO_UPDATE, on).apply();
    }

    /** Wall-clock time (ms) of the last successful update check. */
    long lastCheck() {
        return prefs.getLong(LAST_CHECK, 0);
    }

    void setLastCheck(long millis) {
        prefs.edit().putLong(LAST_CHECK, millis).apply();
    }

    /** Release tag the user declined from an automatic check. */
    String skippedVersion() {
        return prefs.getString(SKIPPED, null);
    }

    void setSkippedVersion(String tag) {
        prefs.edit().putString(SKIPPED, tag).apply();
    }
}
