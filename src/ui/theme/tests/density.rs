// SPDX-License-Identifier: MIT

use crate::{
    test_support::gtk_test,
    ui::{browser_modes::BrowserDensity, preferences::PreferenceManager, theme::ThemeManager},
};

#[test]
fn browser_custom_density_css_loads_updates_and_clears_live() {
    gtk_test(
        "ui::theme::tests::density::browser_custom_density_css_loads_updates_and_clears_live",
        || {
            let themes = ThemeManager::shared();
            let manager = PreferenceManager::shared();

            // Compact/Airy rely entirely on the static stylesheet: no generated rules.
            assert_eq!(manager.browser_density(), BrowserDensity::Compact);
            assert_eq!(themes.density_stylesheet(), "");

            // GTK's CssProvider re-serializes rules (alphabetical selectors/properties),
            // so assert on the values present rather than the exact source text.
            manager.set_browser_density(BrowserDensity::Custom(9));
            let css = themes.density_stylesheet();
            assert!(css.contains("margin-top: 9px"));
            assert!(css.contains("margin-bottom: 9px"));
            assert!(css.contains(".density-custom"));
            assert!(css.contains(".file-row"));

            // A different value replaces the generated rules rather than stacking them.
            manager.set_browser_density(BrowserDensity::Custom(3));
            let css = themes.density_stylesheet();
            assert!(css.contains("margin-top: 3px"));
            assert!(!css.contains("margin-top: 9px"));

            // Re-applying the same value does not accumulate padding: the generated
            // stylesheet stays exactly the same text.
            let before = themes.density_stylesheet();
            manager.set_browser_density(BrowserDensity::Custom(3));
            assert_eq!(themes.density_stylesheet(), before);

            // Switching back to a preset clears the generated provider.
            manager.set_browser_density(BrowserDensity::Airy);
            assert_eq!(themes.density_stylesheet(), "");
        },
    );
}
