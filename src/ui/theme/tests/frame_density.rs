// SPDX-License-Identifier: MIT

use crate::{
    test_support::gtk_test,
    ui::{browser_modes::FrameDensity, preferences::PreferenceManager, theme::ThemeManager},
};

/// Top Bar's three scoped selector groups: header + tab strip, Miller column
/// headers (toolbar/navigation controls), and the preview header/controls.
/// `headerbar.browser-header` and `.preview-header` already appear in the
/// text-size-driven `sizing` block's `min-height` rule regardless of frame
/// density, so presence of the bare selector string isn't a frame-specific
/// signal; `padding-top`/`padding-bottom` are the properties only
/// `frame_density_css` ever emits.
const TOP_BAR_SELECTORS: [&str; 3] = [
    "headerbar.browser-header",
    ".directory-column .column-header",
    ".preview-header",
];

/// Side Bar's one scoped selector group: sidebar rows.
const SIDE_BAR_SELECTOR: &str = ".sidebar-row";

#[test]
fn frame_compact_emits_no_generated_inset_in_either_area_and_matches_todays_chrome() {
    gtk_test(
        "ui::theme::tests::frame_density::frame_compact_emits_no_generated_inset_in_either_area_and_matches_todays_chrome",
        || {
            let themes = ThemeManager::shared();
            let manager = PreferenceManager::shared();

            assert_eq!(manager.frame_top_bar_density(), FrameDensity::Compact);
            assert_eq!(manager.frame_side_bar_density(), FrameDensity::Compact);
            let css = themes.token_stylesheet();
            // Compact matches today's chrome exactly in both areas: no
            // generated vertical inset at all, on top of or independent from
            // the existing text-size-driven min-height rules.
            assert!(!css.contains("padding-top"));
            assert!(!css.contains("padding-bottom"));
        },
    );
}

#[test]
fn frame_airy_adds_exactly_four_px_to_only_its_own_areas_selectors_live() {
    gtk_test(
        "ui::theme::tests::frame_density::frame_airy_adds_exactly_four_px_to_only_its_own_areas_selectors_live",
        || {
            let themes = ThemeManager::shared();
            let manager = PreferenceManager::shared();

            manager.set_frame_top_bar_density(FrameDensity::Airy);
            // GTK's CssProvider re-serializes rules (alphabetical selectors/
            // properties), so assert on the values present rather than exact text.
            let css = themes.token_stylesheet();
            for selector in TOP_BAR_SELECTORS {
                assert!(
                    css.contains(selector),
                    "missing top bar rule for {selector}"
                );
            }
            assert!(css.contains("padding-top: 4px"));
            assert!(css.contains("padding-bottom: 4px"));
            // Top Bar's icon-to-icon gap scales with the same inset.
            assert!(
                css.contains("margin-left: 4px"),
                "Top Bar's icons must gain matching horizontal spacing"
            );
            // Side Bar's own selector got no rule from Top Bar's Airy.
            assert!(
                !css.contains(&format!("{SIDE_BAR_SELECTOR} {{ padding-top")),
                "Top Bar Airy must not inset the Side Bar"
            );

            manager.set_frame_top_bar_density(FrameDensity::Compact);
            manager.set_frame_side_bar_density(FrameDensity::Airy);
            let css = themes.token_stylesheet();
            assert!(css.contains(SIDE_BAR_SELECTOR));
            assert!(css.contains("padding-top: 4px"));
            assert!(css.contains("padding-bottom: 4px"));
            // Side Bar's Airy never adds the Top Bar's icon-spacing rule.
            assert!(
                !css.contains("margin-left"),
                "Side Bar Airy must not add Top Bar's icon spacing"
            );
            for selector in TOP_BAR_SELECTORS {
                assert!(
                    !css.contains(&format!("{selector} "))
                        || !css.contains("padding-top: 4px; padding-bottom: 4px"),
                    "Side Bar Airy must not inset Top Bar's {selector}"
                );
            }

            manager.set_frame_side_bar_density(FrameDensity::Compact);
        },
    );
}

#[test]
fn frame_custom_accepts_bounds_updates_live_and_never_accumulates_per_area() {
    gtk_test(
        "ui::theme::tests::frame_density::frame_custom_accepts_bounds_updates_live_and_never_accumulates_per_area",
        || {
            let themes = ThemeManager::shared();
            let manager = PreferenceManager::shared();

            manager.set_frame_top_bar_density(FrameDensity::custom(7));
            assert_eq!(manager.frame_top_bar_density(), FrameDensity::Custom(7));
            let css = themes.token_stylesheet();
            assert!(css.contains("padding-top: 7px"));
            assert!(
                css.contains("margin-left: 7px"),
                "Top Bar's icon spacing follows the same Custom value"
            );

            // Re-saving the same value does not accumulate padding: the
            // regenerated stylesheet is exactly the same text.
            let before = themes.token_stylesheet();
            manager.set_frame_top_bar_density(FrameDensity::custom(7));
            assert_eq!(themes.token_stylesheet(), before);

            // Out-of-range input clamps into 0..=12 rather than applying or
            // persisting the raw value.
            manager.set_frame_top_bar_density(FrameDensity::custom(200));
            assert_eq!(manager.frame_top_bar_density(), FrameDensity::Custom(12));
            assert!(themes.token_stylesheet().contains("padding-top: 12px"));

            // Boundary 0px is a distinct saved selection (Custom(0), not
            // Compact) but renders identically: a 0px inset has no visible
            // effect, so the generator emits nothing, same as Compact.
            manager.set_frame_top_bar_density(FrameDensity::custom(0));
            assert_eq!(manager.frame_top_bar_density(), FrameDensity::Custom(0));
            assert!(!themes.token_stylesheet().contains("padding-top"));

            manager.set_frame_top_bar_density(FrameDensity::Compact);

            // The Side Bar area independently exercises the same bounds/
            // boundary/no-accumulation behavior.
            manager.set_frame_side_bar_density(FrameDensity::custom(7));
            assert_eq!(manager.frame_side_bar_density(), FrameDensity::Custom(7));
            assert!(themes.token_stylesheet().contains("padding-top: 7px"));

            let before = themes.token_stylesheet();
            manager.set_frame_side_bar_density(FrameDensity::custom(7));
            assert_eq!(themes.token_stylesheet(), before);

            manager.set_frame_side_bar_density(FrameDensity::custom(200));
            assert_eq!(manager.frame_side_bar_density(), FrameDensity::Custom(12));
            assert!(themes.token_stylesheet().contains("padding-top: 12px"));

            manager.set_frame_side_bar_density(FrameDensity::custom(0));
            assert_eq!(manager.frame_side_bar_density(), FrameDensity::Custom(0));
            assert!(!themes.token_stylesheet().contains("padding-top"));

            manager.set_frame_side_bar_density(FrameDensity::Compact);
        },
    );
}

#[test]
fn frame_areas_are_independent_of_each_other_and_of_browser_density() {
    gtk_test(
        "ui::theme::tests::frame_density::frame_areas_are_independent_of_each_other_and_of_browser_density",
        || {
            use crate::ui::browser_modes::BrowserDensity;

            let themes = ThemeManager::shared();
            let manager = PreferenceManager::shared();

            manager.set_frame_top_bar_density(FrameDensity::custom(5));
            manager.set_frame_side_bar_density(FrameDensity::custom(9));
            manager.set_browser_density(BrowserDensity::Custom(11));
            assert_eq!(manager.frame_top_bar_density(), FrameDensity::Custom(5));
            assert_eq!(manager.frame_side_bar_density(), FrameDensity::Custom(9));
            assert_eq!(manager.browser_density(), BrowserDensity::Custom(11));
            let css = themes.token_stylesheet();
            assert!(css.contains("padding-top: 5px"), "Top Bar's own inset");
            assert!(css.contains("padding-top: 9px"), "Side Bar's own inset");
            assert!(themes.density_stylesheet().contains("margin-top: 11px"));

            // Changing one frame area never touches the other area, the
            // browser density value, or its generated stylesheet.
            manager.set_frame_top_bar_density(FrameDensity::Airy);
            assert_eq!(manager.frame_side_bar_density(), FrameDensity::Custom(9));
            assert_eq!(manager.browser_density(), BrowserDensity::Custom(11));
            assert!(themes.token_stylesheet().contains("padding-top: 9px"));
            assert!(themes.density_stylesheet().contains("margin-top: 11px"));

            manager.set_frame_top_bar_density(FrameDensity::Compact);
            manager.set_frame_side_bar_density(FrameDensity::Compact);
            manager.set_browser_density(BrowserDensity::Airy);
        },
    );
}
