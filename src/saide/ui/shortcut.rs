//! Shortcut Manager - Manages keyboard shortcuts and their associated actions.
//!
//! Allows for defining global and scoped shortcuts, handling input, and
//! dispatching actions based on user input.
use {
    super::action::{Action, ActionArgs, Function},
    crate::saide::SAideApp,
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
pub type ShortcutMap = HashMap<Shortcut, Action>;

/// Represents a scope of shortcuts, which can be pushed and popped from a stack.
/// Scopes allow for context-specific shortcuts that can override global ones.
pub struct ShortcutScope {
    pub name: &'static str,
    pub shortcuts: ShortcutMap,

    /// If true, shortcuts in this scope will consume the input event,
    /// preventing it from being handled by lower scopes or global shortcuts.
    pub consume: bool,
}

impl ShortcutScope {
    pub fn new(name: &'static str, shortcuts: ShortcutMap, consume: bool) -> Self {
        Self {
            name,
            shortcuts,
            consume,
        }
    }
}

/// Manages keyboard shortcuts and their associated actions.
pub struct ShortcutManager {
    global: ShortcutMap,
    stack: Vec<ShortcutScope>,
}

impl ShortcutManager {
    /// Creates a new ShortcutManager with the given global shortcuts.
    pub fn new(global: ShortcutMap) -> Self {
        Self {
            global,
            stack: Vec::new(),
        }
    }

    /// Pushes a new shortcut scope onto the stack.
    pub fn push_scope(&mut self, scope: ShortcutScope) { self.stack.push(scope); }

    /// Pops the top shortcut scope from the stack.
    pub fn pop_scope(&mut self) { self.stack.pop(); }

    /// Registers a global shortcut and its associated action.
    pub fn register_global(&mut self, sc: Shortcut, action: Action) {
        self.global.insert(sc, action);
    }

    pub fn dispatch(&self, app: &mut SAideApp, ctx: &Context) -> Result<(), ShortcutError> {
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

fn try_execute(action: &Action, app: &mut SAideApp, ctx: &Context) -> Result<(), ShortcutError> {
    action
        .execute(app, &ActionArgs::Context(ctx))
        .map_err(|e| ShortcutError::ActionFailed(e.to_string()))?;
    Ok(())
}

/// Macro to create a ShortcutMap from a list of key-action pairs.
#[macro_export]
macro_rules! shortcuts {
    ( $( $key:expr => [ $( $action:expr ),+ $(,)? ] );* $(;)? ) => {{

        let mut map = $crate::saide::ui::shortcut::ShortcutMap::new();
        $(
            let actions = vec![ $( $action.into() ),* ];
            debug_assert!(!actions.is_empty(), "Shortcut must have at least one action");

            let action = if actions.len() == 1 {
                actions.into_iter().next().unwrap()
            } else {
                $crate::saide::ui::action::Action::Serial(actions)
            };
            map.insert($key, action);
        )*
        map
    }};
}
