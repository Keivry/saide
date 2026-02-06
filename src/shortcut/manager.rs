//! Shortcut Manager - Manages keyboard shortcuts and their associated actions.
//!
//! Allows for defining global and scoped shortcuts, handling input, and
//! dispatching actions based on user input.
use {
    super::super::action::{Action, ActionArgs},
    egui::{Context, Key, Modifiers},
    std::collections::HashMap,
    thiserror::Error,
};

#[derive(Error, Debug)]
pub enum ShortcutError {
    #[error("Shortcut not found")]
    NotFound,
    #[error("Action execution failed: {0}")]
    ActionFailed(String),
}

/// Represents a keyboard shortcut with a key and modifier keys.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Shortcut {
    pub key: Key,
    pub mods: Modifiers,
}

/// Type alias for a mapping of shortcuts to actions.
pub type ShortcutMap<App> = HashMap<Shortcut, Action<App>>;

/// Represents a scope of shortcuts, which can be pushed and popped from a stack.
/// Scopes allow for context-specific shortcuts that can override global ones.
pub struct ShortcutScope<App> {
    pub name: &'static str,
    pub shortcuts: ShortcutMap<App>,

    /// If true, shortcuts in this scope will consume the input event,
    /// preventing it from being handled by lower scopes or global shortcuts.
    pub consume: bool,
}

impl<App> ShortcutScope<App> {
    pub fn new(name: &'static str, shortcuts: ShortcutMap<App>, consume: bool) -> Self {
        Self {
            name,
            shortcuts,
            consume,
        }
    }
}

/// Manages keyboard shortcuts and their associated actions.
pub struct ShortcutManager<App> {
    global: ShortcutMap<App>,
    stack: Vec<ShortcutScope<App>>,
}

impl<App> ShortcutManager<App> {
    /// Creates a new ShortcutManager with the given global shortcuts.
    pub fn new(global: ShortcutMap<App>) -> Self {
        Self {
            global,
            stack: Vec::new(),
        }
    }

    /// Pushes a new shortcut scope onto the stack.
    pub fn push_scope(&mut self, scope: ShortcutScope<App>) { self.stack.push(scope); }

    /// Pops the top shortcut scope from the stack.
    pub fn pop_scope(&mut self) { self.stack.pop(); }

    /// Registers a global shortcut and its associated action.
    pub fn register_global(&mut self, sc: Shortcut, action: Action<App>) {
        self.global.insert(sc, action);
    }

    pub fn dispatch(&self, app: &mut App, ctx: &Context) -> Result<(), ShortcutError>
    where
        App: 'static,
    {
        let mut result = Ok(());

        ctx.input_mut(|input| {
            let mut consumed = vec![];
            for event in &input.events {
                if let egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } = event
                {
                    let sc = Shortcut {
                        key: *key,
                        mods: *modifiers,
                    };

                    let mut triggered = false;
                    for scope in self.stack.iter().rev() {
                        if let Some(action) = scope.shortcuts.get(&sc) {
                            // Only overwrite result if no error has occurred yet
                            if result.is_ok() {
                                result = try_execute(action, app, ctx);
                            }
                            consumed.push(sc);
                            triggered = true;

                            if scope.consume {
                                break;
                            }
                        }
                    }

                    if triggered {
                        continue;
                    }

                    if let Some(action) = self.global.get(&sc) {
                        // Only overwrite result if no error has occurred yet
                        if result.is_ok() {
                            result = try_execute(action, app, ctx);
                        }
                        consumed.push(sc);
                    }
                }
            }

            consumed.into_iter().for_each(|sc| {
                input.consume_key(sc.mods, sc.key);
            });
        });

        result
    }
}

fn try_execute<App>(action: &Action<App>, app: &mut App, ctx: &Context) -> Result<(), ShortcutError>
where
    App: 'static,
{
    action
        .execute(app, &ActionArgs::Context(ctx))
        .map_err(|e| ShortcutError::ActionFailed(e.to_string()))?;
    Ok(())
}

/// Helper function to create a Shortcut from a string representation.
pub fn shortcut(sc: &str) -> Shortcut {
    let mut mods = Modifiers::default();
    let mut key = None;

    sc.split('+').for_each(|part| {
        let part = part.trim();
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => mods.ctrl = true,
            "alt" => mods.alt = true,
            "shift" => mods.shift = true,
            "meta" | "cmd" | "command" => mods.mac_cmd = true,
            k => key = Key::from_name(k),
        }
    });

    let key = key.expect("Invalid key in shortcut string");
    Shortcut { key, mods }
}

#[macro_export]
macro_rules! sc {
    ($s:literal) => {
        $crate::shortcut::shortcut($s)
    };
}

/// Macro to create a ShortcutMap from a list of key-action pairs.
#[macro_export]
macro_rules! shortcuts {
    (
        $(
            $key:expr => $action:expr
        );* $(;)?
    ) => {{
        let mut map = $crate::shortcut::ShortcutMap::new();
        $(
            map.insert($key, $action);
        )*
        map
    }};
}
