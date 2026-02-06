//! Helper module defining actions for egui-based applications.
//!
//! This module defines a flexible system for representing and executing actions.
//! Actions can be simple UI interactions or more complex
//! functions that manipulate the application's state. The system supports
//! chaining multiple actions together, allowing for complex workflows to be
//! defined in a modular way.
//!
//! Initial designed for keyboard shortcuts.

use {
    egui::{Context, Key, Pos2},
    std::convert::TryFrom,
    thiserror::Error,
};

/// Errors that can occur during action execution
#[derive(Error, Debug)]
pub enum ActionError {
    #[error("Invalid action argument type")]
    InvalidArgType,

    #[error("Action execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Action execution was terminated")]
    Terminated,
}

/// Result of an action execution
pub enum ActionResult {
    None,
    Bool(bool),
    String(String),
    Usize(usize),
    Key(Key),
}
pub type Result<T> = std::result::Result<T, ActionError>;

/// Arguments passed to actions execution
pub enum ActionArgs<'a> {
    None,
    Context(&'a Context),
    Key(&'a Key),
    String(&'a str),
    Usize(usize),
    Pos2(&'a Pos2),
    Multi(Vec<ActionArgs<'a>>),
}

impl<'a> ActionArgs<'a> {
    pub fn as_context(&self) -> Option<&'a Context> {
        match self {
            ActionArgs::Context(ctx) => Some(ctx),
            _ => None,
        }
    }

    pub fn as_key(&self) -> Option<&Key> {
        match self {
            ActionArgs::Key(k) => Some(k),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            ActionArgs::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_usize(&self) -> Option<usize> {
        match self {
            ActionArgs::Usize(u) => Some(*u),
            _ => None,
        }
    }

    pub fn as_pos2(&self) -> Option<&Pos2> {
        match self {
            ActionArgs::Pos2(p) => Some(p),
            _ => None,
        }
    }

    pub fn idx(&self, index: usize) -> Option<&ActionArgs<'a>> {
        match self {
            ActionArgs::Multi(v) => v.get(index),
            _ => None,
        }
    }
}

impl<'a> From<&'a Context> for ActionArgs<'a> {
    fn from(ctx: &'a Context) -> Self { ActionArgs::Context(ctx) }
}

impl<'a> From<&'a Key> for ActionArgs<'a> {
    fn from(k: &'a Key) -> Self { ActionArgs::Key(k) }
}

impl<'a> From<&'a str> for ActionArgs<'a> {
    fn from(s: &'a str) -> Self { ActionArgs::String(s) }
}

impl<'a> From<usize> for ActionArgs<'a> {
    fn from(u: usize) -> Self { ActionArgs::Usize(u) }
}

impl<'a> TryFrom<&'a ActionResult> for ActionArgs<'a> {
    type Error = ActionError;

    /// Convert ActionResult to ActionArgs for serial action execution
    fn try_from(value: &'a ActionResult) -> std::result::Result<Self, Self::Error> {
        match value {
            ActionResult::None => Err(ActionError::Terminated),
            ActionResult::Bool(b) => {
                if *b {
                    Ok(ActionArgs::None)
                } else {
                    Err(ActionError::Terminated)
                }
            }
            ActionResult::String(s) => Ok(ActionArgs::String(s)),
            ActionResult::Usize(u) => Ok(ActionArgs::Usize(*u)),
            ActionResult::Key(k) => Ok(ActionArgs::Key(k)),
        }
    }
}

impl<'a, T1, T2> From<(T1, T2)> for ActionArgs<'a>
where
    T1: Into<ActionArgs<'a>>,
    T2: Into<ActionArgs<'a>>,
{
    fn from(value: (T1, T2)) -> Self { ActionArgs::Multi(vec![value.0.into(), value.1.into()]) }
}

impl<'a, T1, T2, T3> From<(T1, T2, T3)> for ActionArgs<'a>
where
    T1: Into<ActionArgs<'a>>,
    T2: Into<ActionArgs<'a>>,
    T3: Into<ActionArgs<'a>>,
{
    fn from(value: (T1, T2, T3)) -> Self {
        ActionArgs::Multi(vec![value.0.into(), value.1.into(), value.2.into()])
    }
}

impl<'a, T1, T2, T3, T4> From<(T1, T2, T3, T4)> for ActionArgs<'a>
where
    T1: Into<ActionArgs<'a>>,
    T2: Into<ActionArgs<'a>>,
    T3: Into<ActionArgs<'a>>,
    T4: Into<ActionArgs<'a>>,
{
    fn from(value: (T1, T2, T3, T4)) -> Self {
        ActionArgs::Multi(vec![
            value.0.into(),
            value.1.into(),
            value.2.into(),
            value.3.into(),
        ])
    }
}

impl<'a, T1, T2, T3, T4, T5> From<(T1, T2, T3, T4, T5)> for ActionArgs<'a>
where
    T1: Into<ActionArgs<'a>>,
    T2: Into<ActionArgs<'a>>,
    T3: Into<ActionArgs<'a>>,
    T4: Into<ActionArgs<'a>>,
    T5: Into<ActionArgs<'a>>,
{
    fn from(value: (T1, T2, T3, T4, T5)) -> Self {
        ActionArgs::Multi(vec![
            value.0.into(),
            value.1.into(),
            value.2.into(),
            value.3.into(),
            value.4.into(),
        ])
    }
}

/// Trait for Action execution
pub trait Execute<App>: Send + Sync + 'static {
    fn execute(&self, app: &mut App, arg: &ActionArgs) -> Result<ActionResult>;
}

/// Callback type for UI actions
pub type UiCallback<App, T> = Box<dyn Fn(&mut App, &ActionArgs) -> T + Sync + Send + 'static>;

/// Show UI dialogs and return results
pub enum ShowUi<App> {
    /// Show a UI dialog that does not return a value
    Null(UiCallback<App, ()>),

    /// Show a UI dialog that returns a bool
    Bool(UiCallback<App, bool>),

    /// Show a UI dialog that returns an optional usize
    Usize(UiCallback<App, Option<usize>>),

    /// Show a UI dialog that returns an optional Key
    Key(UiCallback<App, Option<Key>>),

    /// Show a UI dialog that returns an optional string
    String(UiCallback<App, Option<String>>),
}

impl<App> Execute<App> for ShowUi<App>
where
    App: 'static,
{
    fn execute(&self, app: &mut App, arg: &ActionArgs) -> Result<ActionResult> {
        match self {
            ShowUi::Null(f) => {
                f(app, arg);
                Ok(ActionResult::None)
            }
            ShowUi::Bool(f) => Ok(ActionResult::Bool(f(app, arg))),
            ShowUi::Usize(f) => Ok(f(app, arg)
                .map(ActionResult::Usize)
                .unwrap_or(ActionResult::None)),
            ShowUi::Key(f) => Ok(f(app, arg)
                .map(ActionResult::Key)
                .unwrap_or(ActionResult::None)),
            ShowUi::String(f) => Ok(f(app, arg)
                .map(ActionResult::String)
                .unwrap_or(ActionResult::None)),
        }
    }
}

/// Wraps a function/closure as an action
pub struct Function<App, T> {
    callback: Box<dyn Fn(&mut App, &ActionArgs) -> T + Send + Sync + 'static>,
}

impl<App, T> Function<App, T> {
    pub fn new(callback: Box<dyn Fn(&mut App, &ActionArgs) -> T + Send + Sync + 'static>) -> Self {
        Self { callback }
    }
}

impl<App, T> Execute<App> for Function<App, T>
where
    App: 'static,
    T: Into<Result<ActionResult>> + 'static,
{
    fn execute(&self, app: &mut App, arg: &ActionArgs) -> Result<ActionResult> {
        (self.callback)(app, arg).into()
    }
}

/// Represents an action that can be executed
pub enum Action<App> {
    Action(Box<dyn Execute<App>>),
    Serial(Vec<Box<dyn Execute<App>>>),
}

impl<App> From<ShowUi<App>> for Action<App>
where
    App: 'static,
{
    fn from(ui: ShowUi<App>) -> Self { Action::Action(Box::new(ui)) }
}

impl<App, T> From<Function<App, T>> for Action<App>
where
    App: 'static,
    T: Into<Result<ActionResult>> + 'static,
{
    fn from(func: Function<App, T>) -> Self { Action::Action(Box::new(func)) }
}

impl<App> From<Vec<Action<App>>> for Action<App>
where
    App: 'static,
{
    fn from(actions: Vec<Action<App>>) -> Self {
        let boxed_actions = actions
            .into_iter()
            .map(|action| match action {
                Action::Action(a) => a,
                Action::Serial(_) => {
                    panic!("Nested serial actions are not supported");
                }
            })
            .collect();
        Action::Serial(boxed_actions)
    }
}

impl<App> Action<App>
where
    App: 'static,
{
    pub fn execute(&self, app: &mut App, arg: &ActionArgs) -> Result<ActionResult> {
        match self {
            Action::Action(action) => action.execute(app, arg),
            Action::Serial(actions) => {
                let mut last_result = ActionResult::None;
                for action in actions {
                    last_result = action.execute(app, &ActionArgs::try_from(&last_result)?)?;
                }
                Ok(last_result)
            }
        }
    }
}

pub trait IntoAction<App> {
    fn into_action(self) -> Action<App>;
}

/// Any Execute can be used directly
impl<App, E> IntoAction<App> for E
where
    E: Execute<App> + 'static,
{
    fn into_action(self) -> Action<App> { Action::Action(Box::new(self)) }
}

/// Helper macro to match action argument types
#[macro_export]
macro_rules! expect_arg {
    ($arg:expr, $default:expr, $pat:pat => $body:expr) => {
        match $arg {
            $pat => $body,
            _ => $default,
        }
    };
}

/// Macro to import action-related types
macro_rules! with_action_types {
    ($($tt:tt)*) => {{
        #[allow(unused_imports)]
        use $crate::action::{ActionArgs, ActionResult, ActionError, Function, Action, ShowUi, IntoAction};
        $($tt)*
    }};
}

/// Macro to create actions in different modes
//// Modes:
/// - ui: for UI dialogs, with optional return types
/// - func: for functions with optional argument types
/// - serial: for chaining multiple actions, action results are passed as arguments to the next
///   action
/// - default: for any expression implementing Execute
///
/// Usage examples:
/// ```ignore
/// action!(ui App::show_help_dialog);
/// action!(ui bool App::show_confirm_dialog);
/// action!(func App::delete_item);
/// action!(func bool App::set_verbose_mode);
/// action!(func key pos App::handle_key_at_position);
/// action!(serial [action1, action2, ...]);
/// action!(custom_execute);
/// ```
#[macro_export]
macro_rules! action {
    // --- ui mode ---
    // Usage: action!(ui <return type> <function>)
    (ui $f:expr) => {
        with_action_types!{
            Action::Action(Box::new(ShowUi::Null(Box::new($f))))
        }
    };
    (ui bool $f:expr) => {
        with_action_types!{
            Action::Action(Box::new(ShowUi::Bool(Box::new($f))))
        }
    };
    (ui usize $f:expr) => {
        with_action_types!{
            Action::Action(Box::new(ShowUi::Usize(Box::new($f))))
        }
    };
    (ui string $f:expr) => {
        with_action_types!{
            Action::Action(Box::new(ShowUi::String(Box::new($f))))
        }
    };
    (ui key $f:expr) => {
        with_action_types!{
            Action::Action(Box::new(ShowUi::Key(Box::new($f))))
        }
    };

    // --- func mode ---
    // Usage: action!(func [argument type] <function>)
    (func $f:expr) => {
        with_action_types!{
            Action::Action(Box::new(Function::new(Box::new(|app, _| {
                ($f)(app);
                Ok(ActionResult::None)
            }))))
        }
    };
    (func bool $f:expr) => {
        with_action_types!{
            Action::Action(Box::new(Function::new(Box::new(|app, arg| {
                match arg {
                    ActionArgs::Bool(b) => { ($f)(app, *b); Ok(ActionResult::None) }
                    _ => Err(ActionError::InvalidArgType),
                }
            }))))
        }
    };
    (func usize $f:expr) => {
        with_action_types!{
            Action::Action(Box::new(Function::new(Box::new(|app, arg| {
                match arg {
                    ActionArgs::Usize(u) => { ($f)(app, *u); Ok(ActionResult::None) }
                    _ => Err(ActionError::InvalidArgType),
                }
            }))))
        }
    };
    (func string $f:expr) => {
        with_action_types!{
            Action::Action(Box::new(Function::new(Box::new(|app, arg| {
                match arg {
                    ActionArgs::String(s) => { ($f)(app, s); Ok(ActionResult::None) }
                    _ => Err(ActionError::InvalidArgType),
                }
            }))))
        }
    };
    (func key $f:expr) => {
        with_action_types!{
            Action::Action(Box::new(Function::new(Box::new(|app, arg| {
                match arg {
                    ActionArgs::Key(k) => { ($f)(app, k); Ok(ActionResult::None) }
                    _ => Err(ActionError::InvalidArgType),
                }
            }))))
        }
    };
    (func key pos $f:expr) => {
        with_action_types!{
            Action::Action(Box::new(Function::new(Box::new(|app, arg| {
                match arg {
                    ActionArgs::Multi(v) if v.len() == 2 => {
                        if let (ActionArgs::Key(k), ActionArgs::Pos2(p)) = (&v[0], &v[1]) {
                            ($f)(app, k, p);
                            Ok(ActionResult::None)
                        } else {
                            Err(ActionError::InvalidArgType)
                        }
                    }
                    _ => Err(ActionError::InvalidArgType),
                }
            }))))
        }
    };

    // --- serial mode ---
    // Usage: action!(serial [<action1>, <action2>, ...])
    (serial [$($a:expr),* $(,)?]) => {
        with_action_types!{
            Action::from(vec![$($a),*])
        }
    };

    // --- default mode ---
    // Usage: action!(<expression implementing Execute>)
    ($e:expr) => {
        with_action_types!{
            IntoAction::into_action($e)
        }
    };
}
