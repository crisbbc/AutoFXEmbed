package com.autofxembed;

import android.app.Activity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.os.Build;
import android.os.Bundle;
import android.widget.Toast;

import java.util.ArrayList;
import java.util.List;

/**
 * Share target: rewrites the shared text and either copies it to the clipboard
 * ("Copy embed link") or immediately opens the system share sheet ("Share embed
 * link", via the {@code .ReshareActivity} alias).
 */
public class ShareActivity extends Activity {
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        Intent intent = getIntent();
        String text = Intent.ACTION_SEND.equals(intent.getAction())
                ? intent.getStringExtra(Intent.EXTRA_TEXT)
                : null;
        String rewritten = transformAndRecord(this, text);

        if (rewritten == null) {
            Toast.makeText(this, R.string.nothing_to_do, Toast.LENGTH_SHORT).show();
        } else if (isReshareAlias()) {
            share(this, rewritten);
        } else {
            copy(this, rewritten);
        }
        finish();
    }

    private boolean isReshareAlias() {
        ComponentName component = getComponentName();
        return component != null && component.getClassName().endsWith("ReshareActivity");
    }

    /** Rewrite {@code text} with saved settings and record any converted links. */
    static String transformAndRecord(Context context, String text) {
        if (text == null) {
            return null;
        }
        Prefs prefs = new Prefs(context);
        String[] result = Native.transform(
                text,
                prefs.selection(),
                prefs.stripTrackers(),
                prefs.rewriteX(),
                prefs.rewriteBsky(),
                prefs.rewriteTiktok(),
                prefs.xTranslateLang(),
                prefs.extraRulesRaw());
        if (result == null) {
            return null;
        }
        List<String[]> links = new ArrayList<>();
        for (int i = 1; i + 1 < result.length; i += 2) {
            links.add(new String[] {result[i], result[i + 1]});
        }
        History.record(context, links);
        return result[0];
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

    /** Open Android's share sheet with {@code text}. */
    static void share(Context context, String text) {
        Intent send = new Intent(Intent.ACTION_SEND);
        send.setType("text/plain");
        send.putExtra(Intent.EXTRA_TEXT, text);
        Intent chooser = Intent.createChooser(send, context.getString(R.string.share_embed_title));
        if (!(context instanceof Activity)) {
            chooser.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        }
        context.startActivity(chooser);
    }
}
