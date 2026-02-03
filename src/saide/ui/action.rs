//! Helper module defining actions for SAideApp
//!
//! This module defines a flexible system for representing and executing actions
//! within the SAideApp. Actions can be simple UI interactions or more complex
//! functions that manipulate the application's state. The system supports
//! chaining multiple actions together, allowing for complex workflows to be
//! defined in a modular way.
//!
//! Initial designed for keyboard shortcuts in SAide.

use {
    super::SAideApp,
    egui::{Context, Key},
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

pub enum ActionArgs<'a> {
    None,
    Context(&'a Context),
    Key(Key),
    KeyAndPos(Key, egui::Pos2),
    String(String),
    Usize(usize),
}

impl<'a> From<&'a Context> for ActionArgs<'a> {
    fn from(ctx: &'a Context) -> Self { ActionArgs::Context(ctx) }
}

impl<'a> From<Key> for ActionArgs<'a> {
    fn from(k: Key) -> Self { ActionArgs::Key(k) }
}

impl<'a> From<(Key, egui::Pos2)> for ActionArgs<'a> {
    fn from(kp: (Key, egui::Pos2)) -> Self { ActionArgs::KeyAndPos(kp.0, kp.1) }
}

impl<'a> From<String> for ActionArgs<'a> {
    fn from(s: String) -> Self { ActionArgs::String(s) }
}

impl<'a> From<usize> for ActionArgs<'a> {
    fn from(u: usize) -> Self { ActionArgs::Usize(u) }
}

impl<'a> TryFrom<ActionResult> for ActionArgs<'a> {
    type Error = ActionError;

    fn try_from(value: ActionResult) -> std::result::Result<Self, Self::Error> {
        match value {
            ActionResult::None => Err(ActionError::InvalidArgType),
            ActionResult::Bool(b) => {
                if b {
                    Ok(ActionArgs::None)
                } else {
                    Err(ActionError::Terminated)
                }
            }
            ActionResult::String(s) => Ok(ActionArgs::String(s)),
            ActionResult::Usize(u) => Ok(ActionArgs::Usize(u)),
            ActionResult::Key(k) => Ok(ActionArgs::Key(k)),
        }
    }
}

pub trait Function {
    fn execute(&self, app: &mut SAideApp, arg: &ActionArgs) -> Result<ActionResult>;
}

#[macro_export]
macro_rules! expect_arg {
    ($arg:expr, $pat:pat => $body:expr) => {
        match $arg {
            $pat => $body,
            _ => Err(ActionError::InvalidArgType),
        }
    };
}

pub enum ShowUi {
    /// Show help dialog
    Help(Box<dyn Fn(&mut SAideApp, &Context)>),

    /// Show add profile dialog, returns the profile name
    AddProfile(Box<dyn Fn(&mut SAideApp, &Context) -> Option<String>>),

    /// Show rename profile dialog, returns the new profile name
    RenameProfile(Box<dyn Fn(&mut SAideApp, &Context) -> Option<String>>),

    /// Show switch profile dialog, returns the profile index
    SwitchProfile(Box<dyn Fn(&mut SAideApp, &Context) -> Option<usize>>),

    /// Show add mapping dialog, returns the key to be mapped
    AddMapping(Box<dyn Fn(&mut SAideApp, &Context) -> Option<Key>>),

    /// Show delete mapping dialog
    DeleteMapping(Box<dyn Fn(&mut SAideApp, &Context) -> bool>),
}

impl Function for ShowUi {
    fn execute(&self, app: &mut SAideApp, arg: &ActionArgs) -> Result<ActionResult> {
        match self {
            ShowUi::Help(f) => expect_arg!(arg, ActionArgs::Context(ctx) => {
                f(app, ctx);
                Ok(ActionResult::None)
            }),
            ShowUi::AddProfile(f) => expect_arg!(arg, ActionArgs::Context(ctx) => {
                Ok(f(app, ctx)
                    .map(ActionResult::String)
                    .unwrap_or(ActionResult::None))
            }),
            ShowUi::RenameProfile(f) => expect_arg!(arg, ActionArgs::Context(ctx) => {
                Ok(f(app, ctx)
                    .map(ActionResult::String)
                    .unwrap_or(ActionResult::None))
            }),
            ShowUi::SwitchProfile(f) => expect_arg!(arg, ActionArgs::Context(ctx) => {
                Ok(f(app, ctx)
                    .map(ActionResult::Usize)
                    .unwrap_or(ActionResult::None))
            }),
            ShowUi::AddMapping(f) => expect_arg!(arg, ActionArgs::Context(ctx) => {
                Ok(f(app, ctx)
                    .map(ActionResult::Key)
                    .unwrap_or(ActionResult::None))
            }),
            ShowUi::DeleteMapping(f) => expect_arg!(arg, ActionArgs::Context(ctx) => {
                Ok(ActionResult::Bool(f(app, ctx)))
            }),
        }
    }
}

type StdResult = std::result::Result<(), Box<dyn std::error::Error>>;

pub enum MappingFunc {
    /// Add a new profile with the given name
    AddProfile(Box<dyn Fn(&mut SAideApp, &str) -> StdResult>),

    /// Delete the current profile
    DeleteProfile(Box<dyn Fn(&mut SAideApp) -> StdResult>),

    /// Rename the current profile to the given name
    RenameProfile(Box<dyn Fn(&mut SAideApp, &str) -> StdResult>),

    /// Save current profile as a new profile with the given name
    SaveProfileAs(Box<dyn Fn(&mut SAideApp, &str) -> StdResult>),

    /// Switch to the next profile
    NextProfile(Box<dyn Fn(&mut SAideApp) -> StdResult>),

    /// Switch to the previous profile
    PreviousProfile(Box<dyn Fn(&mut SAideApp) -> StdResult>),

    /// Switch to the profile at the given index
    SwitchProfile(Box<dyn Fn(&mut SAideApp, usize) -> StdResult>),

    /// Add a new mapping for the given key at the given position
    AddMapping(Box<dyn Fn(&mut SAideApp, &Key, &egui::Pos2) -> StdResult>),

    /// Delete the mapping for the given key
    DeleteMapping(Box<dyn Fn(&mut SAideApp, &Key) -> StdResult>),
}

impl Function for MappingFunc {
    fn execute(&self, app: &mut SAideApp, arg: &ActionArgs) -> Result<ActionResult> {
        match self {
            MappingFunc::AddProfile(f) => expect_arg!(arg, ActionArgs::String(name) => {
                map_result(f(app, name))
            }),
            MappingFunc::DeleteProfile(f) => expect_arg!(arg, ActionArgs::None => {
                map_result(f(app))
            }),
            MappingFunc::RenameProfile(f) => expect_arg!(arg, ActionArgs::String(name) => {
                map_result(f(app, name))
            }),
            MappingFunc::SaveProfileAs(f) => expect_arg!(arg, ActionArgs::String(name) => {
                map_result(f(app, name))
            }),
            MappingFunc::NextProfile(f) => expect_arg!(arg, ActionArgs::None => {
                map_result(f(app))
            }),
            MappingFunc::PreviousProfile(f) => expect_arg!(arg, ActionArgs::None => {
                map_result(f(app))
            }),
            MappingFunc::SwitchProfile(f) => expect_arg!(arg, ActionArgs::Usize(index) => {
                map_result(f(app, *index))
            }),
            MappingFunc::AddMapping(f) => expect_arg!(arg, ActionArgs::KeyAndPos(key, pos) => {
                map_result(f(app, key, pos))
            }),
            MappingFunc::DeleteMapping(f) => expect_arg!(arg, ActionArgs::Key(key) => {
                map_result(f(app, key))
            }),
        }
    }
}

/// An action that can be executed in SAideApp
/// Can be a UI action, a mapping function, or a series of actions
/// to be executed in sequence, passing the result of each action
/// as the argument to the next action.
/// If any action in the series returns Error or Terminated, the execution
/// of the series is stopped.
pub enum Action {
    ShowUi(ShowUi),
    Function(MappingFunc),
    Serial(Vec<Action>),
}

impl From<ShowUi> for Action {
    fn from(ui_action: ShowUi) -> Self { Action::ShowUi(ui_action) }
}

impl From<MappingFunc> for Action {
    fn from(action: MappingFunc) -> Self { Action::Function(action) }
}

impl Function for Action {
    fn execute(&self, app: &mut SAideApp, arg: &ActionArgs) -> Result<ActionResult> {
        match self {
            Action::ShowUi(ui_action) => ui_action.execute(app, arg),
            Action::Function(action) => action.execute(app, arg),
            Action::Serial(actions) => {
                let mut last_result = ActionResult::None;
                for action in actions {
                    last_result = action.execute(app, &ActionArgs::try_from(last_result)?)?;
                }
                Ok(last_result)
            }
        }
    }
}

/// Helper function to map StdResult to Action Result for MappingFunc
fn map_result(r: StdResult) -> Result<ActionResult> {
    r.map(|_| ActionResult::None)
        .map_err(|e| ActionError::ExecutionFailed(e.to_string()))
}
