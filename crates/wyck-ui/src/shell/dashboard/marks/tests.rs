use super::commodity::commodity;
use super::crypto::{crypto, crypto_slug};
use super::forex::currency_mark;
use super::index::index_flag;
use super::paint::is_dark;
use super::shares::share;
use super::*;
use crate::kit::assets::has_mark;

#[test]
fn a_forex_pair_is_two_flags() {
    let icon = icon_for("EURUSD", Class::Forex, Some("EUR"), Some("USD"));
    assert_eq!(icon.primary, Mark::Flag("eu"));
    assert_eq!(icon.secondary, Some(Mark::Flag("us")));
}

#[test]
fn a_pair_falls_back_to_its_ticker_when_the_assets_are_unknown() {
    let icon = icon_for("GBPJPY", Class::Forex, None, None);
    assert_eq!(icon.primary, Mark::Flag("gb"));
    assert_eq!(icon.secondary, Some(Mark::Flag("jp")));
}

#[test]
fn a_currency_without_a_flag_shows_letters() {
    assert!(matches!(currency_mark("XYZ"), Mark::Letters { .. }));
}

#[test]
fn an_index_takes_the_flag_of_its_country() {
    let flag = |t| index_flag(t);
    assert_eq!(flag("US30.cash"), Some("us"));
    assert_eq!(flag("US2000.cash"), Some("us"));
    assert_eq!(flag("GER40.cash"), Some("de"));
    assert_eq!(flag("N25.cash"), Some("nl"));
    assert_eq!(flag("UK100.cash"), Some("gb"));
    assert_eq!(flag("DXY.cash"), Some("us"));
    assert_eq!(flag("NOPE.cash"), None);
}

#[test]
fn metals_carry_their_chemical_symbol_and_the_flag_of_the_quote() {
    let icon = icon_for("XAUEUR", Class::Metals, Some("XAU"), Some("EUR"));
    assert!(matches!(icon.primary, Mark::Metal { symbol: "Au", .. }));
    assert_eq!(icon.secondary, Some(Mark::Flag("eu")));
}

#[test]
fn coins_are_found_under_the_brokers_shortened_names() {
    assert_eq!(crypto_slug("BTCUSD"), Some("btc"));
    assert_eq!(crypto_slug("LNKUSD"), Some("link"));
    assert_eq!(crypto_slug("VECUSD"), Some("vet"));
    assert_eq!(crypto_slug("ZZZUSD"), None);
}

#[test]
fn a_coin_with_a_bundled_logo_uses_it() {
    assert_eq!(crypto("BTCUSD"), Mark::Logo("marks/crypto/btc.svg".into()));
}

#[test]
fn company_logos_accept_broker_suffixes_and_unknowns_keep_letters() {
    assert!(matches!(share("JPM"), Mark::Letters { .. }));
    assert!(matches!(share("AAPL"), Mark::Brand { .. }));
    assert!(matches!(share("aapl.us"), Mark::Brand { .. }));
    assert!(matches!(share("ADSGn.DE"), Mark::Brand { .. }));
    assert!(matches!(share("VOWG_p.DE"), Mark::Brand { .. }));
    for ticker in [
        "ABNB", "AXP", "CAT", "DE", "MA", "NET", "P911", "PYPL", "SHOP", "SONY", "TGT", "WFC",
    ] {
        assert!(matches!(share(ticker), Mark::Brand { .. }), "{ticker}");
    }
}

#[test]
fn physical_products_have_a_class_and_a_specific_mark() {
    for (ticker, class) in [
        ("GOLD", Class::Metals),
        ("XCUUSD", Class::Metals),
        ("ALUMINUM", Class::Metals),
        ("UKOIL.cash", Class::Energies),
        ("BRENT", Class::Energies),
        ("XNGUSD", Class::Energies),
        ("GASOLINE", Class::Energies),
        ("COFFEE.c", Class::Commodities),
        ("SOYBEANS", Class::Commodities),
        ("LUMBER", Class::Commodities),
        ("CATTLE", Class::Commodities),
    ] {
        assert_eq!(known_physical_class(ticker), Some(class), "{ticker}");
        assert!(
            !matches!(
                icon_for(ticker, class, None, None).primary,
                Mark::Letters { .. }
            ),
            "{ticker}"
        );
    }
    assert_eq!(known_physical_class("CL"), None);
    assert_eq!(known_physical_class("CL.c"), Some(Class::Energies));
    assert!(commodity("UNKNOWN.c").is_none());
}

#[test]
fn dark_brand_colors_are_recognized() {
    assert!(is_dark(0x0000_0000));
    assert!(is_dark(0x0011_1111));
    assert!(!is_dark(0x00FF_9900));
}

#[test]
fn every_flag_a_mapping_can_name_is_bundled() {
    let codes = [
        "eu", "us", "gb", "jp", "ch", "au", "nz", "ca", "cn", "hk", "sg", "se", "no", "dk", "pl",
        "cz", "hu", "tr", "za", "mx", "il", "th", "in", "kr", "br", "ru", "de", "fr", "nl", "es",
    ];
    for code in codes {
        assert!(
            has_mark(&format!("marks/flags/{code}.svg")),
            "missing {code}"
        );
    }
}
