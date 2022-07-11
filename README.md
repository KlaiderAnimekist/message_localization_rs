# Message Localization

<p align="center">
  <a href="https://crates.io/crates/recoyx_message_localization">
      <img src="https://img.shields.io/crates/d/recoyx_message_localization" alt="crates.io">
  </a>
  <a href="https://docs.rs/recoyx_message_localization">
      <img src="https://shields.io/badge/-docs-brightgreen.svg" alt="docs.rs">
  </a>
</p>

All-in-one package for flexible localization on Rust.

Features:

- `MessageLocator`
  - Load assets from either HTTP or file system.
- General language code and country code manipulation.
  - `Locale` object and `parse_locale(str)`
  - `Country` object and `parse_country(str)`

## Getting started

This example uses the [Tokio](https://tokio.rs) asynchronous runtime framework, solely for demonstrative purposes.

Add the following dependencies to Cargo.toml:

```toml
[dependencies]
recoyx_localization = "1"
maplit = "1.0"
tokio = { version = "1", features = ["full"] }
```

Example asset located at `res/lang/en/_.json`:

```json
{
    "message_id": "Some message",
    "parameterized": "Here: $x",
    "contextual_male": "Male message",
    "contextual_female": "Female message",
    "contextual_other": "Other message",
    "qty_empty": "Empty ($number)",
    "qty_one": "One ($number)",
    "qty_multiple": "Multiple ($number)"
}
```

Example program using these assets:

```rust
use recoyx_localization::{
    MessageLocator, MessageLocatorOptions, MessageLocatorAssetOptions,
    MessageLocatorLoadVia,
    localization_vars,
};
use maplit::hashmap;

#[tokio::main]
async fn main() {
    let mut msg_locator = MessageLocator::new(
        MessageLocatorOptions::new()
            // Specify supported locale codes.
            // The form in which the locale code appears here
            // is a post-component for the assets "src" path. 
            // For example: "path/to/res/lang/en-US"
            .supported_locales(vec!["en", "en-US", "pt-BR"])
            .default_locale("en-US")
            .fallbacks(hashmap! {
                "en-US" => vec!["en"],
                "pt-BR" => vec!["en-US"],
            })
            .assets(MessageLocatorAssetOptions::new()
                .src("res/lang")
                .base_file_names(vec!["_"])
                // "clean_unused" indicates whether to clean previous unused locale data. 
                .clean_unused(true)
                // Specify MessageLocatorLoadVia::FileSystem or MessageLocatorLoadVia::Http
                .load_via(MessageLocatorLoadVia::FileSystem))
    ); // msg_locator

    if (!msg_locator.load(None).await) {
        // failed to load
    }

    println!("{}", msg_locator.get("_.message_id"));
    println!("{}", msg_locator.get_formatted("_.parameterized", vec![ &localization_vars!{
        "x" => "foo"
    } ]));
    println!("{}", msg_locator.get_formatted("_.contextual", vec![ "female" ]));
}
```