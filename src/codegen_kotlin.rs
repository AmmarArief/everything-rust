//! Kotlin's generated artifacts are Java artifacts.
//!
//! A Kotlin top-level function compiles to a static method on `<File>Kt`, and `@JvmStatic`
//! in an `object` compiles to a static method on that class. Java source can call both, so
//! the Java JNI codegen, batch codegen, reflection driver and shadow generation are reused
//! unchanged. Keeping one generator also keeps generated code under the same clippy gate.

pub use crate::codegen_java::{
    DRIVER_JAVA, ERROR_JAVA, JSON_JAVA, WIRE_JAVA, native_class, shadows, targets, typed_rs,
    wire_rs, wrappers,
};

/// JNI short name of a JVM method: package separators become `_`, and a literal `_`
/// becomes `_1`. Unicode is escaped as `_0xxxx` by javac; Kotlin identifiers in the bridged
/// subset are ASCII, so this covers exported functions.
pub fn jni_symbol(owner: &str, name: &str) -> String {
    let mangle = |s: &str| {
        s.chars()
            .flat_map(|c| match c {
                '.' => "_".chars().collect::<Vec<_>>(),
                '_' => "_1".chars().collect::<Vec<_>>(),
                other => vec![other],
            })
            .collect::<String>()
    };
    format!("Java_{}_{}", mangle(owner), mangle(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mangles_jvm_names() {
        assert_eq!(
            jni_symbol("com.example.NativeLibKt", "add"),
            "Java_com_example_NativeLibKt_add"
        );
        assert_eq!(
            jni_symbol("com.example.my_math.Math", "mul"),
            "Java_com_example_my_1math_Math_mul"
        );
    }
}
