//! The web build's interface language. The browser gives GridCraft no system fonts and the build
//! bundles none for Chinese, Japanese or Korean, so an interface in one of those would draw boxes
//! (tofu) until the host page adds fonts that draw it (`gridcraft:font`, see `host`). Until then
//! the build shows English; once the added fonts draw every East Asian character of that
//! interface's labels it switches (egui's own fonts draw the rest).
//! No browser types here, so it is tested natively; `web` feeds it the browser's language,
//! `?lang=` and the fonts added.

use gridcraft_ui_egui::UiState;
use gridcraft_ui_egui::i18n::{self, Language};
use gridcraft_ui_egui::theme;

/// The interface language to show. One picked by hand (`chosen`) stays; otherwise the page's
/// `?lang=` (`requested`), else the browser's language, when the fonts draw its interface
/// (`draws`), and English when they don't.
pub fn pick(chosen: Option<Language>, requested: Option<Language>, browser: Language, draws: impl FnOnce(Language) -> bool) -> Language {
    if let Some(language) = chosen {
        return language;
    }
    let wanted = requested.unwrap_or(browser);
    if draws(wanted) { wanted } else { Language::En }
}

/// Whether the fonts added so far draw `language`'s interface: always for the languages egui's own
/// fonts draw; for Chinese, Japanese and Korean once the added fonts map every East Asian
/// character their labels use (Hangul; Han; kana and Han, with their punctuation).
pub fn fonts_draw(language: Language) -> bool {
    theme::extra_fonts_draw(i18n::interface_text(language).flat_map(str::chars))
}

/// Keeps the interface language in step with the fonts the host adds.
pub struct FollowFonts {
    requested: Option<Language>,
    browser: Language,
    /// How many added fonts the language was last picked for; `None` before the first pick.
    fonts: Option<usize>,
}

impl FollowFonts {
    pub fn new(requested: Option<Language>, browser: Language) -> Self {
        Self { requested, browser, fonts: None }
    }

    /// Picks the language for the `fonts` fonts added so far (`draws` tells which interfaces they
    /// draw): at startup, then again each time another font is added (other frames cost a
    /// comparison), until someone picks a language by hand ([`UiState::language_chosen`]).
    pub fn update(&mut self, ui: &mut UiState, fonts: usize, draws: impl FnOnce(Language) -> bool) {
        if self.fonts == Some(fonts) {
            return;
        }
        self.fonts = Some(fonts);
        ui.language = pick(ui.language_chosen.then_some(ui.language), self.requested, self.browser, draws);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridcraft_engine::Session;
    use gridcraft_ui_egui::SheetApp;
    use serde_json::json;

    const CJK: [Language; 3] = [Language::Ko, Language::Ja, Language::Zh];
    /// The interfaces egui's own fonts draw.
    const LATIN_AND_CYRILLIC: [Language; 8] =
        [Language::En, Language::Es, Language::Ru, Language::PtBr, Language::De, Language::Cs, Language::Fr, Language::Sr];

    /// Fonts that draw the interfaces in `drawn` (and, like egui's own, every Latin and Cyrillic one).
    fn fonts(drawn: &'static [Language]) -> impl Fn(Language) -> bool {
        move |l| !CJK.contains(&l) || drawn.contains(&l)
    }

    #[test]
    fn a_cjk_browser_language_shows_english_until_a_font_draws_it() {
        for browser in CJK {
            assert_eq!(pick(None, None, browser, fonts(&[])), Language::En, "{browser:?} with no font added");
            assert_eq!(pick(None, None, browser, fonts(&[Language::Ko, Language::Ja, Language::Zh])), browser, "a pan-CJK font draws {browser:?}");
        }
        assert_eq!(pick(None, None, Language::Ko, fonts(&[Language::Ko])), Language::Ko);
        for browser in LATIN_AND_CYRILLIC {
            assert_eq!(pick(None, None, browser, fonts(&[])), browser, "{browser:?} needs no added font");
        }
    }

    #[test]
    fn each_cjk_interface_needs_fonts_that_draw_its_own_labels() {
        // A Korean font maps some Han and kana too (Hanja, KS X 1001's kana), but not the Simplified
        // Chinese or Japanese interface: those browsers stay in English with it.
        let korean = fonts(&[Language::Ko]);
        assert_eq!(pick(None, None, Language::Zh, &korean), Language::En);
        assert_eq!(pick(None, None, Language::Ja, &korean), Language::En);
        let japanese = fonts(&[Language::Ja]);
        assert_eq!(pick(None, None, Language::Ja, &japanese), Language::Ja);
        assert_eq!(pick(None, None, Language::Ko, &japanese), Language::En, "a Japanese font doesn't draw Korean");
        assert_eq!(pick(None, None, Language::Zh, fonts(&[Language::Zh])), Language::Zh);
    }

    #[test]
    fn the_page_can_ask_for_a_language_on_the_same_terms() {
        let korean = fonts(&[Language::Ko]);
        assert_eq!(pick(None, Some(Language::Ko), Language::En, &korean), Language::Ko, "`?lang=` wins over the browser");
        assert_eq!(pick(None, Some(Language::Ko), Language::Ko, fonts(&[])), Language::En, "no font draws it yet");
        assert_eq!(pick(None, Some(Language::Ko), Language::De, fonts(&[])), Language::En, "English meanwhile, not the browser's language");
        assert_eq!(pick(None, Some(Language::De), Language::Ko, &korean), Language::De, "a Latin language needs no font");
        assert_eq!(pick(None, Some(Language::Ja), Language::Ko, &korean), Language::En, "a Korean font doesn't draw the Japanese asked for");
    }

    #[test]
    fn a_language_picked_by_hand_stays() {
        let korean = fonts(&[Language::Ko]);
        assert_eq!(pick(Some(Language::De), None, Language::Ko, &korean), Language::De);
        assert_eq!(pick(Some(Language::En), Some(Language::Ko), Language::Ko, &korean), Language::En);
        assert_eq!(pick(Some(Language::Ko), None, Language::En, fonts(&[])), Language::Ko, "even before a font draws it");
    }

    #[test]
    fn the_language_follows_the_fonts_added_until_someone_picks_one() {
        let mut app = SheetApp::new(Session::new(), Default::default());
        let mut follow = FollowFonts::new(None, Language::Ko);
        follow.update(&mut app.ui, 0, fonts(&[]));
        assert_eq!(app.ui.language, Language::En, "no font yet");
        follow.update(&mut app.ui, 1, fonts(&[Language::Ja]));
        assert_eq!(app.ui.language, Language::En, "a Japanese font doesn't draw Korean");
        let mut asked = false;
        follow.update(&mut app.ui, 1, |_| {
            asked = true;
            true
        });
        assert!(!asked, "the fonts are only asked again once another one was added");
        follow.update(&mut app.ui, 2, fonts(&[Language::Ja, Language::Ko]));
        assert_eq!(app.ui.language, Language::Ko, "a font that draws Korean switches the interface to it");

        // A language picked from the View tab (or File › Options) stays when more fonts arrive.
        app.run("app.language.set", json!({"language": "de"})).unwrap();
        follow.update(&mut app.ui, 3, fonts(&[Language::Ja, Language::Ko]));
        assert_eq!(app.ui.language, Language::De);
    }

    #[test]
    fn picking_the_language_already_shown_counts_as_a_choice() {
        // The Korean font is late: the user, seeing English, picks English.
        let mut app = SheetApp::new(Session::new(), Default::default());
        let mut follow = FollowFonts::new(Some(Language::Ko), Language::Ko);
        follow.update(&mut app.ui, 0, fonts(&[]));
        assert_eq!(app.ui.language, Language::En);
        app.run("app.language.english", json!({})).unwrap();
        follow.update(&mut app.ui, 1, fonts(&[Language::Ko]));
        assert_eq!(app.ui.language, Language::En, "the font doesn't override the choice");
    }

    #[test]
    fn without_cjk_fonts_the_page_starts_in_its_language_or_english() {
        let mut ui = UiState::default();
        FollowFonts::new(None, Language::De).update(&mut ui, 0, fonts(&[]));
        assert_eq!(ui.language, Language::De, "the browser's language");
        let mut ui = UiState::default();
        FollowFonts::new(Some(Language::Ko), Language::De).update(&mut ui, 0, fonts(&[]));
        assert_eq!(ui.language, Language::En, "the page asked for Korean, which nothing draws yet");
    }

    /// The only test here that adds a font (they are process-wide): the checks run in order.
    #[test]
    fn the_added_fonts_decide_which_interfaces_draw() {
        for language in LATIN_AND_CYRILLIC {
            assert!(fonts_draw(language), "{language:?} needs no added font");
        }
        for language in CJK {
            assert!(!fonts_draw(language), "{language:?} with no font added");
        }
        // A Korean font whose transfer broke off after its character map: it maps the whole Korean
        // interface but has no outlines to draw it with, so it is refused and a Korean browser
        // stays in English (with it, the labels' Hangul would draw blank).
        let cut = theme::add_font("Cut Off", cut_off_after_its_character_map(Language::Ko), 0);
        assert!(cut.as_ref().is_err_and(|e| e.contains("glyph outlines")), "{cut:?}");
        assert!(!fonts_draw(Language::Ko));
        let mut ui = UiState::default();
        FollowFonts::new(None, Language::Ko).update(&mut ui, theme::extra_font_count(), fonts_draw);
        assert_eq!(ui.language, Language::En);

        let Some((bytes, index, draws_zh_and_ja)) = korean_face() else {
            return; // no Korean face on this machine
        };
        let added = theme::add_font("Korean", bytes, index);
        assert!(added.as_ref().is_ok_and(|a| a.added), "{added:?}");
        assert!(fonts_draw(Language::Ko), "a Korean face draws the Korean interface");
        // A Korean-only face has Hanja and kana enough for `covers` to say Han and kana, but not
        // the Simplified Chinese or Japanese labels; a pan-CJK collection's Korean face has both.
        if let Some(expected) = draws_zh_and_ja {
            assert_eq!(fonts_draw(Language::Zh), expected, "{added:?}");
            assert_eq!(fonts_draw(Language::Ja), expected, "{added:?}");
        }
    }

    /// A Korean face installed on this machine, if any, and whether it draws the Chinese and
    /// Japanese interfaces too (when that is known for the font).
    fn korean_face() -> Option<(Vec<u8>, u32, Option<bool>)> {
        let win = std::env::var("WINDIR").map(|w| format!("{w}\\Fonts")).unwrap_or_else(|_| "C:\\Windows\\Fonts".into());
        let candidates = [
            ("/System/Library/Fonts/AppleSDGothicNeo.ttc".to_string(), 0, Some(false)),
            (format!("{win}\\malgun.ttf"), 0, None),
            ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc".to_string(), 1, Some(true)),
            ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc".to_string(), 1, Some(true)),
            ("/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc".to_string(), 1, Some(true)),
        ];
        candidates.into_iter().find_map(|(path, index, draws_zh_and_ja)| {
            let bytes = std::fs::read(path).ok()?;
            // One too large to add, or that egui can't read, counts as missing.
            (bytes.len() <= theme::MAX_EXTRA_FONT && theme::font_parses(&bytes, index)).then_some((bytes, index, draws_zh_and_ja))
        })
    }

    /// The start of a font whose transfer broke off after its character map: a `head` (1,000 units
    /// per em) and a `cmap` taking every character of `language`'s interface to glyph 1, and none
    /// of the glyph outlines that come after them in most fonts.
    fn cut_off_after_its_character_map(language: Language) -> Vec<u8> {
        let mut chars: Vec<u32> = i18n::interface_text(language).flat_map(str::chars).map(u32::from).collect();
        chars.sort_unstable();
        chars.dedup();
        let groups = u32::try_from(chars.len()).unwrap();
        // One format 12 subtable (Windows, full Unicode), a group per character.
        let mut cmap: Vec<u8> = [0_u16, 1, 3, 10].into_iter().flat_map(u16::to_be_bytes).collect();
        cmap.extend(12_u32.to_be_bytes());
        cmap.extend([12_u16, 0].into_iter().flat_map(u16::to_be_bytes));
        cmap.extend([16 + 12 * groups, 0, groups].into_iter().flat_map(u32::to_be_bytes));
        cmap.extend(chars.into_iter().flat_map(|c| [c, c, 1]).flat_map(u32::to_be_bytes));
        // Version 1.0, the magic number, units per em.
        let mut head = vec![0_u8; 54];
        head[1] = 1;
        head[12..16].copy_from_slice(&0x5F0F_3CF5_u32.to_be_bytes());
        head[18..20].copy_from_slice(&1_000_u16.to_be_bytes());
        // The sfnt header (TrueType, two tables), their records in tag order, then the tables.
        let mut font: Vec<u8> = 0x0001_0000_u32.to_be_bytes().to_vec();
        font.extend([2_u16, 32, 1, 0].into_iter().flat_map(u16::to_be_bytes));
        let mut tables: Vec<u8> = Vec::new();
        for (tag, table) in [(b"cmap", &cmap), (b"head", &head)] {
            let offset = u32::try_from(12 + 2 * 16 + tables.len()).unwrap();
            font.extend(tag);
            font.extend([0, offset, u32::try_from(table.len()).unwrap()].into_iter().flat_map(u32::to_be_bytes));
            tables.extend(table);
            tables.resize(tables.len().next_multiple_of(4), 0);
        }
        font.extend(tables);
        font
    }
}
