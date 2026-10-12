pub mod events;
pub mod executor;
pub mod plan;

pub mod compilation;
pub mod compile;
pub mod manifest;
mod output;
pub mod packages;
mod project;
mod session;
mod walk;
mod workspace;

pub use compilation::SourceOwner;
pub use packages::{
    ConflictingSource, DiscoveredPackage, DiscoveredPackages, PackagesError, SourceOwnership,
    discover_packages,
};
pub use workspace::{Workspace, WorkspaceError};

pub use project::{
    BuildConfig, BuildError, ExecutionError, InitializedProject, PreparedProject, ProjectConfig,
    ProjectError, RunConfig, TestConfig, build, initialize_project, prepare_project,
    resolve_output, run, test,
};
pub use session::{
    BuildSession, BuildSessionConfig, InputChange, InputChanges, RebuildOutcome, SessionError,
    SessionSnapshot, SourceFile, SourceKind,
};
