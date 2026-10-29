package com.autofxembed;

import android.app.Activity;
import android.graphics.Typeface;
import android.os.Bundle;
import android.text.TextUtils;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.view.inputmethod.EditorInfo;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.RadioButton;
import android.widget.ScrollView;
import android.widget.Switch;
import android.widget.TextView;

import java.util.ArrayList;
import java.util.List;

/**
 * Launcher screen: explains the share flow, picks the X / Twitter target
 * (built-in or custom), and shows the Recent list and update settings.
 */
public class SettingsActivity extends Activity {
    private Prefs prefs;
    private LinearLayout targets;
    private LinearLayout recent;
    private Button checkNow;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        prefs = new Prefs(this);

        LinearLayout column = new LinearLayout(this);
        column.setOrientation(LinearLayout.VERTICAL);
        int pad = dp(20);
        column.setPadding(pad, pad, pad, pad);

        column.addView(text(getString(R.string.app_name), 24));
        TextView intro = text(getText(R.string.intro), 16);
        intro.setPadding(0, dp(8), 0, 0);
        column.addView(intro);

        column.addView(heading(getString(R.string.x_target)));
        targets = new LinearLayout(this);
        targets.setOrientation(LinearLayout.VERTICAL);
        column.addView(targets);
        addDomainRow(column);
        column.addView(note(getString(R.string.target_warning)));

        column.addView(heading(getString(R.string.settings)));
        column.addView(switchRow(getString(R.string.strip_trackers), prefs.stripTrackers(),
                prefs::setStripTrackers));
        column.addView(note(getString(R.string.strip_trackers_summary)));

        LinearLayout recentHeader = row();
        recentHeader.setPadding(0, dp(24), 0, 0);
        TextView recentTitle = heading(getString(R.string.recent));
        recentTitle.setPadding(0, 0, 0, 0);
        recentHeader.addView(recentTitle, new LinearLayout.LayoutParams(0, -2, 1));
        Button clear = flatButton(getString(R.string.clear));
        clear.setOnClickListener(v -> History.clear(this));
        recentHeader.addView(clear);
        column.addView(recentHeader);
        recent = new LinearLayout(this);
        recent.setOrientation(LinearLayout.VERTICAL);
        column.addView(recent);

        column.addView(heading(getString(R.string.updates)));
        column.addView(switchRow(getString(R.string.auto_update), prefs.autoUpdate(),
                prefs::setAutoUpdate));
        checkNow = flatButton(getString(R.string.check_now));
        checkNow.setGravity(Gravity.START | Gravity.CENTER_VERTICAL);
        // Line the label up with the text above it.
        checkNow.setPadding(0, checkNow.getPaddingTop(), 0, checkNow.getPaddingBottom());
        checkNow.setMinWidth(0);
        checkNow.setOnClickListener(v -> {
            checkNow.setEnabled(false);
            checkNow.setText(R.string.checking);
            Updater.check(this, true, this::resetCheckButton);
        });
        column.addView(checkNow);
        column.addView(note(getString(R.string.version, Updater.currentVersion(this))));

        ScrollView scroll = new ScrollView(this);
        // Keep content clear of the status and navigation bars (edge-to-edge).
        scroll.setFitsSystemWindows(true);
        scroll.addView(column);
        setContentView(scroll);

        renderTargets();
        Updater.checkIfDue(this);
    }

    @Override
    protected void onResume() {
        super.onResume();
        History.setOnChange(this::renderRecent);
        renderRecent();
        resetCheckButton();
        History.fetchPending(this);
    }

    @Override
    protected void onPause() {
        History.setOnChange(null);
        super.onPause();
    }

    private void resetCheckButton() {
        boolean busy = Updater.isBusy();
        checkNow.setEnabled(!busy);
        checkNow.setText(busy ? R.string.checking : R.string.check_now);
    }

    // ---- X / Twitter targets ----

    private void renderTargets() {
        targets.removeAllViews();
        String selected = prefs.selection();
        List<RadioButton> buttons = new ArrayList<>();
        String[] builtins = Native.targets();
        for (int i = 0; i + 1 < builtins.length; i += 2) {
            targets.addView(choice(buttons, builtins[i], builtins[i + 1], selected));
        }
        for (String domain : prefs.customDomains()) {
            LinearLayout row = row();
            String id = Prefs.CUSTOM_PREFIX + domain;
            row.addView(choice(buttons, id, domain, selected), new LinearLayout.LayoutParams(0, -2, 1));
            Button remove = flatButton("✕");
            remove.setContentDescription(getString(R.string.remove_domain, domain));
            remove.setOnClickListener(v -> {
                prefs.removeCustomDomain(domain);
                renderTargets();
            });
            row.addView(remove);
            targets.addView(row);
        }
    }

    /** A radio button for selection {@code id}; checking it unchecks the rest. */
    private RadioButton choice(List<RadioButton> group, String id, String label, String selected) {
        RadioButton button = new RadioButton(this);
        button.setText(label);
        button.setTextSize(TypedValue.COMPLEX_UNIT_SP, 16);
        button.setMinHeight(dp(48));
        button.setChecked(id.equals(selected));
        button.setOnCheckedChangeListener((b, checked) -> {
            if (!checked) {
                return;
            }
            prefs.setSelection(id);
            for (RadioButton other : group) {
                if (other != b) {
                    other.setChecked(false);
                }
            }
        });
        group.add(button);
        return button;
    }

    private void addDomainRow(LinearLayout column) {
        LinearLayout row = row();
        EditText input = new EditText(this);
        input.setHint(R.string.domain_hint);
        input.setSingleLine(true);
        input.setInputType(EditorInfo.TYPE_CLASS_TEXT | EditorInfo.TYPE_TEXT_VARIATION_URI);
        input.setImeOptions(EditorInfo.IME_ACTION_DONE);
        row.addView(input, new LinearLayout.LayoutParams(0, -2, 1));
        Button add = flatButton(getString(R.string.add));
        row.addView(add);
        column.addView(row);
        TextView error = note(null);
        error.setTextColor(0xFFE53935);
        error.setAlpha(1f);
        error.setVisibility(View.GONE);
        column.addView(error);

        Runnable submit = () -> {
            String[] result = Native.normalizeDomain(input.getText().toString());
            if (result[0] == null) {
                error.setText(result[1]);
                error.setVisibility(View.VISIBLE);
                return;
            }
            error.setVisibility(View.GONE);
            input.setText("");
            prefs.addCustomDomain(result[0]);
            renderTargets();
        };
        add.setOnClickListener(v -> submit.run());
        input.setOnEditorActionListener((v, action, event) -> {
            submit.run();
            return true;
        });
    }

    // ---- Recent ----

    private void renderRecent() {
        recent.removeAllViews();
        List<History.Entry> entries = History.recent(this, History.SHOWN_ENTRIES);
        if (entries.isEmpty()) {
            recent.addView(note(getString(R.string.recent_empty)));
            return;
        }
        for (History.Entry entry : entries) {
            LinearLayout item = new LinearLayout(this);
            item.setOrientation(LinearLayout.VERTICAL);
            item.setPadding(0, dp(8), 0, dp(8));
            item.setMinimumHeight(dp(48));
            TypedValue ripple = new TypedValue();
            getTheme().resolveAttribute(android.R.attr.selectableItemBackground, ripple, true);
            item.setBackgroundResource(ripple.resourceId);
            TextView label = text(Native.label(entry.title, entry.description, entry.embed), 16);
            label.setSingleLine(true);
            label.setEllipsize(TextUtils.TruncateAt.END);
            item.addView(label);
            TextView url = note(entry.embed);
            url.setSingleLine(true);
            url.setEllipsize(TextUtils.TruncateAt.MIDDLE);
            item.addView(url);
            item.setOnClickListener(v -> ShareActivity.copy(this, entry.embed));
            item.setContentDescription(getString(R.string.copy_link, label.getText()));
            recent.addView(item);
        }
    }

    // ---- views ----

    private interface Toggle {
        void set(boolean on);
    }

    private Switch switchRow(String label, boolean on, Toggle toggle) {
        Switch view = new Switch(this);
        view.setText(label);
        view.setTextSize(TypedValue.COMPLEX_UNIT_SP, 16);
        view.setMinHeight(dp(48));
        view.setChecked(on);
        view.setOnCheckedChangeListener((button, checked) -> toggle.set(checked));
        return view;
    }

    private LinearLayout row() {
        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        row.setGravity(Gravity.CENTER_VERTICAL);
        return row;
    }

    private Button flatButton(String label) {
        Button button = new Button(this, null, android.R.attr.borderlessButtonStyle);
        button.setText(label);
        button.setAllCaps(false);
        return button;
    }

    private TextView heading(String value) {
        TextView view = text(value, 18);
        view.setTypeface(Typeface.DEFAULT_BOLD);
        view.setPadding(0, dp(24), 0, dp(4));
        return view;
    }

    private TextView note(CharSequence value) {
        TextView view = text(value, 13);
        view.setAlpha(0.7f);
        return view;
    }

    private TextView text(CharSequence value, int sp) {
        TextView view = new TextView(this);
        view.setTextAppearance(android.R.style.TextAppearance_Material_Body1);
        view.setText(value);
        view.setTextSize(TypedValue.COMPLEX_UNIT_SP, sp);
        return view;
    }

    private int dp(int value) {
        return Math.round(TypedValue.applyDimension(
                TypedValue.COMPLEX_UNIT_DIP, value, getResources().getDisplayMetrics()));
    }
}
