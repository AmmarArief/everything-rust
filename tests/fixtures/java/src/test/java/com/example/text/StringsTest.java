package com.example.text;

class StringsTest {
    void slugify() {
        assert Strings.slugify("Hello, World!", 0).equals("hello-world");
    }
}
