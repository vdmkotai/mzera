/* Optional Omarchy template: copy to ~/.config/omarchy/themed/mzera.css.tpl to override
   the CSS mzera generates from colors.toml. Rendered to
   ~/.local/state/omarchy/current/theme/mzera.css on every theme change. */
window.mz { background-color: {{ background }}; color: {{ foreground }}; }
.mz-gallery-bg { background-color: {{ dark_background }}; }
.mz-pill { background-color: {{ background }}; border: 1px solid {{ lighter_background }}; border-radius: 999px; padding: 4px 8px; }
.mz-pill button.on, .mz-pill button:checked { background-color: {{ mix accent background 30% }}; border-color: {{ accent }}; }
