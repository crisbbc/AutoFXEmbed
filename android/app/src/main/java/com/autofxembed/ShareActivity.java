package com.autofxembed;

import android.app.Activity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.os.Build;
import android.os.Bundle;
import android.widget.Toast;

import java.util.ArrayList;
import java.util.List;

/**
 * Share target: rewrites the shared text and puts it on the clipboard.
 * Android stops background apps from reading the clipboard, so sharing is
 * how links reach the app here.
 */
public class ShareActivity extends Activity {
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        Intent intent = getIntent();
        String text = Intent.ACTION_SEND.equals(intent.getAction())
                ? intent.getStringExtra(Intent.EXTRA_TEXT)
                : null;
        Prefs prefs = new Prefs(this);
        String[] result = text == null
                ? null
                : Native.transform(text, prefs.selection(), prefs.stripTrackers());

        if (result == null) {
            Toast.makeText(this, R.string.nothing_to_do, Toast.LENGTH_SHORT).show();
        } else {
            copy(this, result[0]);
            List<String[]> links = new ArrayList<>();
            for (int i = 1; i + 1 < result.length; i += 2) {
                links.add(new String[] {result[i], result[i + 1]});
            }
            History.record(this, links);
        }
        finish();
    }

    /** Put {@code text} on the clipboard, with a toast where Android shows none. */
    static void copy(Context context, String text) {
        ClipboardManager clipboard = context.getSystemService(ClipboardManager.class);
        clipboard.setPrimaryClip(ClipData.newPlainText(context.getString(R.string.app_name), text));
        // Android 13+ confirms clipboard writes itself.
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
            Toast.makeText(context, R.string.copied, Toast.LENGTH_SHORT).show();
        }
    }
}
