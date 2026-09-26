//! The newline-delimited JSON that clients and the watcher exchange.
//!
//! Each line is one JSON object. A request carries a client-chosen `id`, the `query`'s name, and
//! the query's parameters as further fields; its response repeats the `id`. The shape is
//! maintained within Iris and may change between releases.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: Value,
    #[serde(flatten)]
    pub query: Query,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "query", rename_all = "lowercase")]
pub enum Query {
    /// Rescan sources, rebuild if anything changed, and answer with the build outcome.
    Wait,
    /// The signature of a value, or the kind of a type or class.
    Signature {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        namespace: Option<Namespace>,
    },
    /// A module's exports with their signatures and documentation.
    Module { name: String },
    /// Where a value, type, or class is declared.
    Definition {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        namespace: Option<Namespace>,
    },
    /// Where a value, type, or class is used.
    References {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        namespace: Option<Namespace>,
    },
    /// The modules that import a module, directly or through other modules.
    Dependents { name: String },
    /// The instances of a class, or the instances whose head mentions a type.
    Instances { name: String, search: InstanceSearch },
    /// The diagnostics of one module, or of every module.
    Diagnostics { name: Option<String> },
    /// The JavaScript generated for a module.
    Javascript { name: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub id: Value,
    #[serde(flatten)]
    pub body: ResponseBody,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ResponseBody {
    /// The answer, computed from build `generation`.
    Result { generation: u64, value: Value },
    /// A change to the watcher's inputs was applied while the query ran. Retry.
    Cancelled,
    /// The request failed.
    Error { message: String },
}

/// Which of the items sharing a name a query is about. Without one, a query is about both the
/// value and the type or class with that name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Namespace {
    /// Values, including functions, data constructors, and class members.
    Value,
    /// Types and classes.
    Type,
}

/// Which instances an `instances` query finds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstanceSearch {
    /// The instances of the named class.
    Class,
    /// The instances whose head mentions the named type, whatever their class.
    Type,
}

impl Response {
    /// Serializes the response as one line, including the trailing newline.
    pub fn to_line(&self) -> String {
        let mut line = serde_json::to_string(self)
            .expect("invariant violated: a response failed to serialize");
        line.push('\n');
        line
    }
}
