use saide::{i18n::L10N, t, tf};

fn main() {
    println!("=== Testing i18n ===\n");

    // Test basic translation
    println!("App title: {}", t!("app-title"));
    println!("Editor: {}", t!("toolbar-editor"));

    // Test translation with arguments
    println!("\nWith arguments:");
    println!("{}", tf!("indicator-fps", "fps" => "60"));
    println!("{}", tf!("indicator-latency", "ms" => "42"));

    // Check current locale
    let locale = L10N.read().current_locale().to_string();
    let is_chinese = L10N.read().is_chinese();
    println!("\nCurrent locale: {}", locale);
    println!("Is Chinese: {}", is_chinese);

    // Switch locale and test again
    println!("\n=== Switching to Chinese ===\n");
    L10N.write().set_locale("zh-CN");

    println!("App title: {}", t!("app-title"));
    println!("Editor: {}", t!("toolbar-editor"));
    println!("{}", tf!("indicator-fps", "fps" => "60"));
    println!("{}", tf!("indicator-latency", "ms" => "42"));

    // Switch back to English
    println!("\n=== Switching to English ===\n");
    L10N.write().set_locale("en-US");

    println!("App title: {}", t!("app-title"));
    println!("Editor: {}", t!("toolbar-editor"));
    println!("{}", tf!("indicator-fps", "fps" => "60"));
    println!("{}", tf!("indicator-latency", "ms" => "42"));

    println!("\n=== i18n test completed ===");
}
