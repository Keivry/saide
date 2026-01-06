use saide::{i18n::L10N, t, tf};

fn main() {
    println!("=== Testing i18n ===\n");

    // Test basic translation
    println!("App title: {}", t!("app-title"));
    println!("App starting: {}", t!("app-starting"));

    // Test translation with arguments
    println!("\nWith arguments:");
    println!("{}", tf!("config-video-backend", "backend" => "Vulkan"));
    println!("{}", tf!("config-max-fps", "fps" => "60"));

    // Check current locale
    let locale = L10N.read().current_locale().to_string();
    let is_chinese = L10N.read().is_chinese();
    println!("\nCurrent locale: {}", locale);
    println!("Is Chinese: {}", is_chinese);

    // Switch locale and test again
    println!("\n=== Switching to Chinese ===\n");
    L10N.write().set_locale("zh-CN");

    println!("App title: {}", t!("app-title"));
    println!("App starting: {}", t!("app-starting"));
    println!("{}", tf!("config-video-backend", "backend" => "Vulkan"));
    println!("{}", tf!("config-max-fps", "fps" => "60"));

    // Switch back to English
    println!("\n=== Switching to English ===\n");
    L10N.write().set_locale("en-US");

    println!("App title: {}", t!("app-title"));
    println!("App starting: {}", t!("app-starting"));
    println!("{}", tf!("config-video-backend", "backend" => "Vulkan"));
    println!("{}", tf!("config-max-fps", "fps" => "60"));

    println!("\n=== i18n test completed ===");
}
