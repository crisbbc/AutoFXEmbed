package com.autofxembed;

/** Shared logic from {@code autofxembed-core}, through {@code android/rust}. */
final class Native {
    static {
        System.loadLibrary("autofxembed");
    }

    private Native() {}

    /**
     * Rewrites every supported link in {@code text}. {@code selection} is a
     * target id from {@link #targets()} or {@code custom:<domain>}. Returns
     * {@code null} when nothing changed, else {@code [newText, original0,
     * embed0, original1, embed1, ...]} for the links rewritten to an embed host.
     */
    static native String[] transform(String text, String selection, boolean stripTrackers);

    /** Built-in X / Twitter targets as {@code [id0, label0, id1, label1, ...]}. */
    static native String[] targets();

    /** {@code [domain, null]} for a usable custom embed domain, else {@code [null, reason]}. */
    static native String[] normalizeDomain(String input);

    /**
     * Embed data of a fetched page as {@code [site, title, description, image]},
     * or {@code null} when the page is not a usable embed of {@code embed}.
     */
    static native String[] parseEmbed(String embed, String finalUrl, String html);

    /** One-line Recent-list label; {@code title} and {@code description} may be null. */
    static native String label(String title, String description, String fallback);

    /** True if release tag {@code latest} is a higher version than {@code current}. */
    static native boolean isNewer(String latest, String current);
}
