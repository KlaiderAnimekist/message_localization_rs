use std::{cell::{Cell, RefCell}, collections::{HashMap, HashSet}, rc::Rc};
use super::*;
use maplit::{hashmap, hashset};
use lazy_static::lazy_static;
use lazy_regex::regex;

/// Creates a `HashMap<String, String>` from a list of key-value pairs.
/// This is based on the [`maplit`](https://github.com/bluss/maplit) crate.
///
/// ## Example
///
/// ```
/// use recoyx_message_localization::localization_vars;
/// fn main() {
///     let map = localization_vars!{
///         "a" => "foo",
///         "b" => "bar",
///     };
///     assert_eq!(map[&"a".to_string()], "foo");
///     assert_eq!(map[&"b".to_string()], "bar");
///     assert_eq!(map.get(&"c".to_string()), None);
/// }
/// ```
#[macro_export]
macro_rules! localization_vars {
    (@single $($x:tt)*) => (());
    (@count $($rest:expr),*) => (<[()]>::len(&[$(localization_vars!(@single $rest)),*]));

    ($($key:expr => $value:expr,)+) => { localization_vars!($($key => $value),+) };
    ($($key:expr => $value:expr),*) => {
        {
            let _cap = localization_vars!(@count $($key),*);
            let mut _map = ::std::collections::HashMap::<String, String>::with_capacity(_cap);
            $(
                let _ = _map.insert($key.to_string(), $value.to_string());
            )*
            _map
        }
    };
}

/// Flexible locale mapping with support for loading message resources.
pub struct MessageLocator {
    _current_locale: Option<Locale>,
    _locale_path_components: Rc<HashMap<Locale, String>>,
    _supported_locales: Rc<HashSet<Locale>>,
    _default_locale: Locale,
    _fallbacks: Rc<HashMap<Locale, Vec<Locale>>>,
    _assets: Rc<HashMap<Locale, serde_json::Value>>,
    _assets_src: String,
    _assets_base_file_names: Vec<String>,
    _assets_clean_unused: bool,
    _assets_load_via: MessageLocatorLoadVia,
}

impl MessageLocator {
    /// Constructs a `MessageLocator` object.
    pub fn new(options: &MessageLocatorOptions) -> Self {
        let mut locale_path_components = HashMap::<Locale, String>::new();
        let mut supported_locales = HashSet::<Locale>::new();
        for code in options._supported_locales.borrow().iter() {
            let locale_parse = parse_locale(code).unwrap();
            locale_path_components.insert(locale_parse.clone(), code.clone());
            supported_locales.insert(locale_parse);
        }
        let mut fallbacks = HashMap::<Locale, Vec<Locale>>::new();
        for (k, v) in options._fallbacks.borrow().iter() {
            fallbacks.insert(parse_locale(k).unwrap(), v.iter().map(|s| parse_locale(s).unwrap()).collect());
        }
        let default_locale = options._default_locale.borrow().clone();
        Self {
            _current_locale: None,
            _locale_path_components: Rc::new(locale_path_components),
            _supported_locales: Rc::new(supported_locales),
            _default_locale: parse_locale(&default_locale).unwrap(),
            _fallbacks: Rc::new(fallbacks),
            _assets: Rc::new(HashMap::new()),
            _assets_src: options._assets.borrow()._src.borrow().clone(),
            _assets_base_file_names: options._assets.borrow()._base_file_names.borrow().iter().map(|s| s.clone()).collect(),
            _assets_clean_unused: options._assets.borrow()._clean_unused.get(),
            _assets_load_via: options._assets.borrow()._load_via.get(),
        }
    }

    /// Returns a set of supported locale codes, reflecting
    /// the ones that were specified when constructing the `MessageLocator`.
    pub fn supported_locales(&self) -> HashSet<Locale> {
        self._supported_locales.as_ref().clone()
    }

    /// Returns `true` if the locale is one of the supported locales
    /// that were specified when constructing the `MessageLocator`,
    /// otherwise `false`.
    pub fn supports_locale(&self, arg: &Locale) -> bool {
        self._supported_locales.contains(arg)
    }

    /// Returns the currently loaded locale.
    pub fn current_locale(&self) -> Option<Locale> {
        self._current_locale.clone()
    }

    /// Returns the currently loaded locale followed by its fallbacks or empty if no locale is loaded.
    pub fn current_locale_seq(&self) -> HashSet<Locale> {
        if let Some(c) = self.current_locale() {
            let mut r: HashSet<Locale> = hashset![c.clone()];
            self.enumerate_fallbacks(c.clone(), &mut r);
            return r;
        }
        hashset![]
    }

    /// Attempts to load the specified locale and its fallbacks.
    /// If any resource fails to load, the method returns `false`, otherwise `true`.
    pub async fn update_locale(&mut self, new_locale: Locale) -> bool {
        self.load(Some(new_locale)).await
    }

    /// Attempts to load a locale and its fallbacks.
    /// If the locale argument is specified, it is loaded.
    /// Otherwise, if there is a default locale, it is loaded, and if not,
    /// the method panics.
    ///
    /// If any resource fails to load, the method returns `false`, otherwise `true`.
    pub async fn load(&mut self, mut new_locale: Option<Locale>) -> bool {
        if new_locale.is_none() { new_locale = Some(self._default_locale.clone()); }
        let new_locale = new_locale.unwrap();
        if !self.supports_locale(&new_locale) {
            panic!("Unsupported locale {}", new_locale.standard_tag());
        }
        let mut to_load: HashSet<Locale> = hashset![new_locale.clone()];
        self.enumerate_fallbacks(new_locale.clone(), &mut to_load);

        let mut new_assets: HashMap<Locale, serde_json::Value> = hashmap![];
        for locale in to_load {
            let res = self.load_single_locale(&locale).await;
            if res.is_none() {
                return false;
            }
            new_assets.insert(locale.clone(), res.unwrap());
        }
        if self._assets_clean_unused {
            Rc::get_mut(&mut self._assets).unwrap().clear();
        }

        for (locale, root) in new_assets {
            Rc::get_mut(&mut self._assets).unwrap().insert(locale, root);
        }
        self._current_locale = Some(new_locale.clone());
        // let new_locale_code = unic_langid::LanguageIdentifier::from_bytes(new_locale.clone().standard_tag().to_string().as_ref()).unwrap();

        true
    }

    async fn load_single_locale(&self, locale: &Locale) -> Option<serde_json::Value> {
        let mut r = serde_json::Value::Object(serde_json::Map::new());
        match self._assets_load_via {
            MessageLocatorLoadVia::FileSystem => {
                for base_name in self._assets_base_file_names.iter() {
                    let locale_path_comp = self._locale_path_components.get(locale);
                    if locale_path_comp.is_none() {
                        panic!("Fallback locale is not supported a locale: {}", locale.standard_tag().to_string());
                    }
                    let res_path = format!("{}/{}/{}.json", self._assets_src, locale_path_comp.unwrap(), base_name);
                    let content = std::fs::read(res_path.clone());
                    if content.is_err() {
                        println!("Failed to load resource at {}.", res_path);
                        return None;
                    }
                    MessageLocator::apply_deep(base_name, serde_json::from_str(String::from_utf8(content.unwrap()).unwrap().as_ref()).unwrap(), &mut r);
                }
            },
            MessageLocatorLoadVia::Http => {
                for base_name in self._assets_base_file_names.iter() {
                    let locale_path_comp = self._locale_path_components.get(locale);
                    if locale_path_comp.is_none() {
                        panic!("Fallback locale is not supported a locale: {}", locale.standard_tag().to_string());
                    }
                    let res_path = format!("{}/{}/{}.json", self._assets_src, locale_path_comp.unwrap(), base_name);
                    let content = reqwest::get(reqwest::Url::parse(res_path.clone().as_ref()).unwrap()).await;
                    if content.is_err() {
                        println!("Failed to load resource at {}.", res_path);
                        return None;
                    }
                    let content = if content.is_ok() { Some(content.unwrap().text().await) } else { None };
                    MessageLocator::apply_deep(base_name, serde_json::from_str(content.unwrap().unwrap().as_ref()).unwrap(), &mut r);
                }
            },
        }
        Some(r)
    }

    fn apply_deep(name: &String, assign: serde_json::Value, mut output: &mut serde_json::Value) {
        let mut names: Vec<&str> = name.split("/").collect();
        let last_name = names.pop();
        for name in names {
            let r = output.get(name);
            if r.is_none() || r.unwrap().as_object().is_none() {
                let r = serde_json::Value::Object(serde_json::Map::new());
                output.as_object_mut().unwrap().insert(String::from(name), r);
            }
            output = output.get_mut(name).unwrap();
        }
        output.as_object_mut().unwrap().insert(String::from(last_name.unwrap()), assign);
    }

    fn enumerate_fallbacks(&self, locale: Locale, output: &mut HashSet<Locale>) {
        for list in self._fallbacks.get(&locale).iter() {
            for item in list.iter() {
                output.insert(item.clone());
                self.enumerate_fallbacks(item.clone(), output);
            }
        }
    }

    /// Retrieves message by identifier.
    pub fn get<S: ToString>(&self, id: S) -> String {
        self.get_formatted(id, vec![])
    }

    /// Retrieves message by identifier with formatting arguments.
    pub fn get_formatted<S: ToString>(&self, id: S, options: Vec<&dyn MessageLocatorFormatArgument>) -> String {
        let mut variables: Option<HashMap<String, String>> = None;
        let mut id = id.to_string();

        for option in options.iter() {
            if let Some(r) = option.as_str() {
                id.push('_');
                id.push_str(r);
            }
            else if let Some(r) = option.as_string() {
                id.push('_');
                id.push_str(r.as_str());
            }
            else if let Some(r) = option.as_string_map() {
                variables = Some(r.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
            }
        }

        if variables.is_none() { variables = Some(HashMap::new()); }
        let variables = variables.unwrap();

        let id: Vec<String> = id.split(".").map(|s| s.to_string()).collect();
        if self._current_locale.is_none() {
            return id.join(".");
        }
        let r = self.get_formatted_with_locale(self._current_locale.clone().unwrap(), &id, &variables);
        if let Some(r) = r { r } else { id.join(".") }
    }

    fn get_formatted_with_locale(&self, locale: Locale, id: &Vec<String>, vars: &HashMap<String, String>) -> Option<String> {
        let message = self.resolve_id(self._assets.get(&locale), id);
        if message.is_some() {
            return Some(self.apply_message(message.unwrap(), vars));
        }

        let fallbacks = self._fallbacks.get(&locale);
        if fallbacks.is_some() {
            for fl in fallbacks.unwrap().iter() {
                let r = self.get_formatted_with_locale(fl.clone(), id, vars);
                if r.is_some() {
                    return r;
                }
            }
        }
        None
    }

    fn apply_message(&self, message: String, vars: &HashMap<String, String>) -> String {
        // regex!(r"\$(\$|[A-Za-z0-9_-]+)").replace_all(&message, R { _vars: vars }).as_ref().to_string()
        regex!(r"\$(\$|[A-Za-z0-9_-]+)").replace_all(&message, |s: &regex::Captures<'_>| {
            let s = s.get(0).unwrap().as_str();
            if s == "$$" {
                "$"
            } else {
                let v = vars.get(&s.to_string().replace("$", ""));
                if let Some(v) = v { v } else { "undefined" }
            }
        }).as_ref().to_string()
    }

    fn resolve_id(&self, root: Option<&serde_json::Value>, id: &Vec<String>) -> Option<String> {
        let mut r = root;
        for frag in id.iter() {
            if r.is_none() {
                return None;
            }
            r = r.unwrap().get(frag);
        }
        if r.is_none() {
            return None;
        }
        let r = r.unwrap().as_str();
        if let Some(r) = r { Some(r.to_string()) } else { None }
    }
}

impl Clone for MessageLocator {
    /// Clones the locator, sharing the same
    /// resources.
    fn clone(&self) -> Self {
        Self {
            _current_locale: self._current_locale.clone(),
            _locale_path_components: self._locale_path_components.clone(),
            _supported_locales: self._supported_locales.clone(),
            _default_locale: self._default_locale.clone(),
            _fallbacks: self._fallbacks.clone(),
            _assets: self._assets.clone(),
            _assets_src: self._assets_src.clone(),
            _assets_base_file_names: self._assets_base_file_names.clone(),
            _assets_clean_unused: self._assets_clean_unused,
            _assets_load_via: self._assets_load_via,
        }
    }
}

pub trait MessageLocatorFormatArgument {
    fn as_str(&self) -> Option<&'static str> { None }
    fn as_string(&self) -> Option<String> { None }
    fn as_string_map(&self) -> Option<HashMap<String, String>> { None }
}

impl MessageLocatorFormatArgument for &'static str {
    fn as_str(&self) -> Option<&'static str> { Some(self) }
}

impl MessageLocatorFormatArgument for String {
    fn as_string(&self) -> Option<String> { Some(self.clone()) }
}

impl MessageLocatorFormatArgument for HashMap<String, String> {
    fn as_string_map(&self) -> Option<HashMap<String, String>> { Some(self.clone()) }
}

impl MessageLocatorFormatArgument for i8 { fn as_string(&self) -> Option<String> { Some(self.to_string()) } }
impl MessageLocatorFormatArgument for i16 { fn as_string(&self) -> Option<String> { Some(self.to_string()) } }
impl MessageLocatorFormatArgument for i32 { fn as_string(&self) -> Option<String> { Some(self.to_string()) } }
impl MessageLocatorFormatArgument for i64 { fn as_string(&self) -> Option<String> { Some(self.to_string()) } }
impl MessageLocatorFormatArgument for i128 { fn as_string(&self) -> Option<String> { Some(self.to_string()) } }
impl MessageLocatorFormatArgument for isize { fn as_string(&self) -> Option<String> { Some(self.to_string()) } }
impl MessageLocatorFormatArgument for u8 { fn as_string(&self) -> Option<String> { Some(self.to_string()) } }
impl MessageLocatorFormatArgument for u16 { fn as_string(&self) -> Option<String> { Some(self.to_string()) } }
impl MessageLocatorFormatArgument for u32 { fn as_string(&self) -> Option<String> { Some(self.to_string()) } }
impl MessageLocatorFormatArgument for u64 { fn as_string(&self) -> Option<String> { Some(self.to_string()) } }
impl MessageLocatorFormatArgument for u128 { fn as_string(&self) -> Option<String> { Some(self.to_string()) } }
impl MessageLocatorFormatArgument for usize { fn as_string(&self) -> Option<String> { Some(self.to_string()) } }
impl MessageLocatorFormatArgument for f32 { fn as_string(&self) -> Option<String> { Some(self.to_string()) } }
impl MessageLocatorFormatArgument for f64 { fn as_string(&self) -> Option<String> { Some(self.to_string()) } }

pub struct MessageLocatorOptions {
    _default_locale: RefCell<String>,
    _supported_locales: RefCell<Vec<String>>,
    _fallbacks: RefCell<HashMap<String, Vec<String>>>,
    _assets: RefCell<MessageLocatorAssetOptions>,
}

impl MessageLocatorOptions {
    pub fn new() -> Self {
        MessageLocatorOptions {
            _default_locale: RefCell::new("en".to_string()),
            _supported_locales: RefCell::new(vec!["en".to_string()]),
            _fallbacks: RefCell::new(hashmap! {}),
            _assets: RefCell::new(MessageLocatorAssetOptions::new()),
        }
    }

    pub fn default_locale<S: ToString>(&self, value: S) -> &Self {
        self._default_locale.replace(value.to_string());
        self
    }

    pub fn supported_locales<S: ToString>(&self, list: Vec<S>) -> &Self {
        self._supported_locales.replace(list.iter().map(|name| name.to_string()).collect());
        self
    }

    pub fn fallbacks<S: ToString>(&self, map: HashMap<S, Vec<S>>) -> &Self {
        self._fallbacks.replace(map.iter().map(|(k, v)| (
            k.to_string(),
            v.iter().map(|s| s.to_string()).collect()
        )).collect());
        self
    }

    pub fn assets(&self, options: &MessageLocatorAssetOptions) -> &Self {
        self._assets.replace(options.clone());
        self
    }
}

pub struct MessageLocatorAssetOptions {
    _src: RefCell<String>,
    _base_file_names: RefCell<Vec<String>>,
    _clean_unused: Cell<bool>,
    _load_via: Cell<MessageLocatorLoadVia>,
}

impl Clone for MessageLocatorAssetOptions {
    fn clone(&self) -> Self {
        Self {
            _src: self._src.clone(),
            _base_file_names: self._base_file_names.clone(),
            _clean_unused: self._clean_unused.clone(),
            _load_via: self._load_via.clone(),
        }
    }
}

impl MessageLocatorAssetOptions {
    pub fn new() -> Self {
        MessageLocatorAssetOptions {
            _src: RefCell::new("res/lang".to_string()),
            _base_file_names: RefCell::new(vec![]),
            _clean_unused: Cell::new(true),
            _load_via: Cell::new(MessageLocatorLoadVia::Http),
        }
    }
    
    pub fn src<S: ToString>(&self, src: S) -> &Self {
        self._src.replace(src.to_string());
        self
    } 

    pub fn base_file_names<S: ToString>(&self, list: Vec<S>) -> &Self {
        self._base_file_names.replace(list.iter().map(|name| name.to_string()).collect());
        self
    }

    pub fn clean_unused(&self, value: bool) -> &Self {
        self._clean_unused.set(value);
        self
    }

    pub fn load_via(&self, value: MessageLocatorLoadVia) -> &Self {
        self._load_via.set(value);
        self
    }
}

#[derive(Copy, Clone)]
pub enum MessageLocatorLoadVia {
    FileSystem,
    Http,
}