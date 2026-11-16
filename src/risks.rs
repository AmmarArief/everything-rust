//! The catalog of TypeScript/JavaScript behaviors that do not carry over to Rust as-is.

use serde::Serialize;

/// A construct whose JS semantics differ from the obvious Rust translation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Risk {
    StringOffsets,
    RegexSyntax,
    DynamicRegex,
    DynamicCode,
    DynamicTyping,
    OrderedCollections,
    Exceptions,
    Async,
    Generators,
    ClassInheritance,
    Reflection,
    Callbacks,
    NumberSemantics,
    Json,
    /// Unbounded or differently sized integers (Python `int`, C/Go/Java widths and overflow).
    IntegerSemantics,
    /// Floor division and modulo that differ from Rust's (`//`, `%` on negatives, C `/` by zero).
    DivisionSemantics,
    /// Iteration over hash sets or maps whose order a port cannot reproduce.
    UnorderedIteration,
    /// Raw pointers, pointer arithmetic, manual memory.
    Pointers,
    /// Threads, goroutines, locks, shared mutable state.
    Concurrency,
    /// Null references that any object value may hold (Java, Go nil, C NULL).
    NullReferences,
    /// Changing classes, modules or methods at runtime (monkeypatching, `method_missing`).
    Monkeypatching,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
}

impl Severity {
    pub fn weight(self) -> f64 {
        match self {
            Self::Low => 1.0,
            Self::Medium => 3.0,
            Self::High => 6.0,
        }
    }
}

impl Risk {
    pub fn severity(self) -> Severity {
        match self {
            Self::StringOffsets | Self::RegexSyntax | Self::DynamicRegex | Self::DynamicCode => {
                Severity::High
            }
            Self::DynamicTyping
            | Self::OrderedCollections
            | Self::Async
            | Self::Generators
            | Self::ClassInheritance
            | Self::Reflection
            | Self::Callbacks => Severity::Medium,
            Self::Exceptions | Self::NumberSemantics | Self::Json => Severity::Low,
            Self::Pointers | Self::Monkeypatching | Self::UnorderedIteration => Severity::High,
            Self::IntegerSemantics
            | Self::DivisionSemantics
            | Self::Concurrency
            | Self::NullReferences => Severity::Medium,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::StringOffsets => "String offsets and slicing",
            Self::RegexSyntax => "Regex features the `regex` crate lacks",
            Self::DynamicRegex => "Regexes built at runtime",
            Self::DynamicCode => "Dynamic code (eval, new Function, computed import)",
            Self::DynamicTyping => "Dynamic typing",
            Self::OrderedCollections => "Insertion-ordered maps and object keys",
            Self::Exceptions => "Exceptions",
            Self::Async => "Async and promises",
            Self::Generators => "Generators",
            Self::ClassInheritance => "Class inheritance",
            Self::Reflection => "Reflection and prototypes",
            Self::Callbacks => "Callback-shaped APIs",
            Self::NumberSemantics => "Number semantics",
            Self::Json => "JSON round-trips",
            Self::IntegerSemantics => "Integer width and overflow",
            Self::DivisionSemantics => "Division and modulo",
            Self::UnorderedIteration => "Hash iteration order",
            Self::Pointers => "Pointers and manual memory",
            Self::Concurrency => "Concurrency",
            Self::NullReferences => "Null references",
            Self::Monkeypatching => "Runtime changes to code",
        }
    }

    pub fn guidance(self) -> &'static str {
        match self {
            Self::StringOffsets => {
                "JS indexes strings in UTF-16 code units, Rust in UTF-8 bytes. Offsets that cross \
                 the boundary must be converted, or non-ASCII text gets wrong ranges silently."
            }
            Self::RegexSyntax => {
                "Lookaround, backreferences and the `y` flag are not in the `regex` crate. Use \
                 `fancy-regex` for those patterns, or rewrite them."
            }
            Self::DynamicRegex => {
                "Patterns come from data or config in JS syntax. Rust regex syntax differs, so \
                 user-visible behavior can change. Pin it with tests on real patterns."
            }
            Self::DynamicCode => {
                "eval, new Function and computed imports have no Rust equivalent. Keep this part \
                 in JS or replace it with an explicit registry."
            }
            Self::DynamicTyping => {
                "Values whose shape is checked at runtime need an explicit Rust enum or \
                 `serde_json::Value`."
            }
            Self::OrderedCollections => {
                "JS Map, Set and object keys keep insertion order. `HashMap` does not: use \
                 `indexmap` wherever order reaches output. With serde_json's `preserve_order`, \
                 `Map::remove` swaps the last key into the hole: use `shift_remove` for JS `delete`. \
                 Sort with `js::sort` when a comparator can return NaN: the order then depends \
                 on V8's algorithm."
            }
            Self::Exceptions => {
                "Thrown errors become `Result`. Errors crossing the bridge must map back to the \
                 same JS error shape."
            }
            Self::Async => {
                "Promises become futures. Bridge with napi-rs `async fn` or `wasm-bindgen-futures`, \
                 or keep the async shell in JS and port the sync core."
            }
            Self::Generators => {
                "Lazy generators become iterators, or return a collected Vec across the bridge."
            }
            Self::ClassInheritance => {
                "Class hierarchies become traits plus composition; `instanceof` checks become enums."
            }
            Self::Reflection => {
                "Proxy, Reflect, Symbol and prototype tricks need a redesign, not a translation."
            }
            Self::Callbacks => {
                "Function-typed parameters and methods cross the bridge on every call. Keep \
                 boundaries coarse: pass data in, get data out."
            }
            Self::NumberSemantics => {
                "JS numbers are f64; bitwise ops truncate to i32; parseInt is lenient. Pick Rust \
                 integer types deliberately."
            }
            Self::Json => {
                "JSON.stringify key order and number formatting must match `serde_json` output if \
                 the result is user-visible."
            }
            Self::IntegerSemantics => {
                "Python ints never overflow; C, Go and Java ints wrap or are undefined on overflow. \
                 Pick i64, i128 or a bignum crate deliberately, and use wrapping_* where the source wraps."
            }
            Self::DivisionSemantics => {
                "Python `//` and `%` floor toward negative infinity; Rust `/` and `%` truncate: use \
                 `div_euclid`/`rem_euclid` or explicit floor math. C division by zero is undefined."
            }
            Self::UnorderedIteration => {
                "Set and map iteration order comes from the source language's hash function. A port \
                 cannot reproduce it: sort before output, or make the order part of the contract."
            }
            Self::Pointers => {
                "Pointer arithmetic and manual memory become slices, indices and owned types. Code that \
                 exposes pointers across the boundary needs `unsafe`, isolated and explained."
            }
            Self::Concurrency => {
                "Threads, goroutines and locks need a design in Rust (channels, Send/Sync), not a \
                 line-by-line translation; keep concurrency in the caller where possible."
            }
            Self::NullReferences => {
                "Any reference may be null in the source: decide per parameter whether it is `Option` \
                 and what null means, instead of mapping every value to `Option`."
            }
            Self::Monkeypatching => {
                "Code that rewrites classes or modules at runtime has no Rust equivalent: keep it in \
                 the source language and port the stable parts."
            }
        }
    }
}

/// Whether a JS regex uses features the `regex` crate does not support.
pub fn regex_needs_fancy(pattern: &str, flags: &str) -> bool {
    if flags.contains('y') {
        return true;
    }
    if ["(?=", "(?!", "(?<=", "(?<!", "\\k<"]
        .iter()
        .any(|needle| pattern.contains(needle))
    {
        return true;
    }
    let bytes = pattern.as_bytes();
    let mut in_class = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                if !in_class && bytes.get(i + 1).is_some_and(|b| (b'1'..=b'9').contains(b)) {
                    return true;
                }
                i += 1;
            }
            b'[' => in_class = true,
            b']' => in_class = false,
            _ => {}
        }
        i += 1;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::regex_needs_fancy;

    #[test]
    fn detects_unsupported_regex_features() {
        assert!(regex_needs_fancy("(?<=a)b", ""));
        assert!(regex_needs_fancy("(a)\\1", ""));
        assert!(regex_needs_fancy("a", "y"));
        assert!(!regex_needs_fancy("[\\w\\d]+?", "g"));
        assert!(!regex_needs_fancy("(?<name>x)", "d"));
        assert!(!regex_needs_fancy("[\\1]", ""));
    }
}
