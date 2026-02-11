// TODO: Implement a notifier that can display messages to the user, such as error messages,
// warnings, or informational messages. This could be done using a simple popup window or a more
// complex notification system depending on the requirements of the application.
pub struct Notifier {}

impl Notifier {
    pub fn new() -> Self { Self {} }

    pub fn notify(&mut self, _message: &str) {
        unimplemented!("Notifier::notify is not implemented yet");
    }
}
