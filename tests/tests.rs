use recoyx_message_localization::*;
use futures_await_test::async_test;

#[test]
fn locale_country() {
    let some_lang = parse_locale(&"pt-BR").unwrap();
    let some_country = some_lang.country();
    assert_eq!(some_lang.to_string(), String::from("Português (Brazil)"));
    assert_eq!(some_lang.standard_tag().to_string(), String::from("pt-BR"));
    assert!(some_country.is_some());
    assert_eq!(some_country.unwrap().standard_code().alpha3(), "BRA");
}

#[tokio::test]
async fn msg_locator() {
    let mut msg_locator = MessageLocator::new(
        MessageLocatorOptions::new()
            .supported_locales(vec!["en-US"])
            .default_locale("en-US")
            .assets(MessageLocatorAssetOptions::new()
                .src("./tests/res/lang")
                .base_file_names(vec!["_"])
                .clean_unused(true)
                .load_via(MessageLocatorLoadVia::FileSystem))
    ); // msg_locator
    msg_locator.load(None).await;
    assert!(msg_locator.supports_locale(&parse_locale("en-US").unwrap()));
    assert_eq!(msg_locator.get("_.message_id"), "Some message".to_string());
}