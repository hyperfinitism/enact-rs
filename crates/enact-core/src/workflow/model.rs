// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;

/// All GitHub Actions event types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
#[clap(rename_all = "snake_case")]
pub enum Event {
    BranchProtectionRule,
    CheckRun,
    CheckSuite,
    Create,
    Delete,
    Deployment,
    DeploymentStatus,
    Discussion,
    DiscussionComment,
    Fork,
    Gollum,
    IssueComment,
    Issues,
    Label,
    MergeGroup,
    Milestone,
    PageBuild,
    Project,
    ProjectCard,
    ProjectColumn,
    PublicEvent,
    PullRequest,
    PullRequestComment,
    PullRequestReview,
    PullRequestReviewComment,
    PullRequestTarget,
    Push,
    RegistryPackage,
    Release,
    RepositoryDispatch,
    Schedule,
    Status,
    Watch,
    WorkflowCall,
    WorkflowDispatch,
    WorkflowRun,
}

impl Event {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BranchProtectionRule => "branch_protection_rule",
            Self::CheckRun => "check_run",
            Self::CheckSuite => "check_suite",
            Self::Create => "create",
            Self::Delete => "delete",
            Self::Deployment => "deployment",
            Self::DeploymentStatus => "deployment_status",
            Self::Discussion => "discussion",
            Self::DiscussionComment => "discussion_comment",
            Self::Fork => "fork",
            Self::Gollum => "gollum",
            Self::IssueComment => "issue_comment",
            Self::Issues => "issues",
            Self::Label => "label",
            Self::MergeGroup => "merge_group",
            Self::Milestone => "milestone",
            Self::PageBuild => "page_build",
            Self::Project => "project",
            Self::ProjectCard => "project_card",
            Self::ProjectColumn => "project_column",
            Self::PublicEvent => "public",
            Self::PullRequest => "pull_request",
            Self::PullRequestComment => "pull_request_comment",
            Self::PullRequestReview => "pull_request_review",
            Self::PullRequestReviewComment => "pull_request_review_comment",
            Self::PullRequestTarget => "pull_request_target",
            Self::Push => "push",
            Self::RegistryPackage => "registry_package",
            Self::Release => "release",
            Self::RepositoryDispatch => "repository_dispatch",
            Self::Schedule => "schedule",
            Self::Status => "status",
            Self::Watch => "watch",
            Self::WorkflowCall => "workflow_call",
            Self::WorkflowDispatch => "workflow_dispatch",
            Self::WorkflowRun => "workflow_run",
        }
    }
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Top-level GitHub Actions workflow.
#[derive(Debug, Deserialize)]
pub struct Workflow {
    pub name: Option<String>,
    pub on: Option<Trigger>,
    pub env: Option<HashMap<String, String>>,
    pub defaults: Option<Defaults>,
    pub jobs: HashMap<String, Job>,
}

/// Workflow trigger — supports single event, list, or map.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Trigger {
    Single(String),
    Multiple(Vec<String>),
    Map(HashMap<String, Option<TriggerConfig>>),
}

impl Trigger {
    pub fn events(&self) -> Vec<String> {
        match self {
            Trigger::Single(s) => vec![s.clone()],
            Trigger::Multiple(v) => v.clone(),
            Trigger::Map(m) => m.keys().cloned().collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TriggerConfig {
    pub branches: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub paths: Option<Vec<String>>,
    #[serde(rename = "paths-ignore")]
    pub paths_ignore: Option<Vec<String>>,
    pub types: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct Defaults {
    pub run: Option<RunDefaults>,
}

#[derive(Debug, Deserialize)]
pub struct RunDefaults {
    pub shell: Option<String>,
    #[serde(rename = "working-directory")]
    pub working_directory: Option<String>,
}

/// A job in the workflow.
#[derive(Debug, Deserialize)]
pub struct Job {
    pub name: Option<String>,
    #[serde(rename = "runs-on")]
    pub runs_on: Option<RunsOn>,
    pub needs: Option<JobNeeds>,
    pub steps: Option<Vec<Step>>,
    pub env: Option<HashMap<String, String>>,
    #[serde(rename = "if")]
    pub condition: Option<String>,
    pub container: Option<Container>,
    pub services: Option<HashMap<String, ServiceContainer>>,
    pub strategy: Option<Strategy>,
    #[serde(rename = "timeout-minutes")]
    pub timeout_minutes: Option<u64>,
    #[serde(rename = "continue-on-error")]
    pub continue_on_error: Option<bool>,
    pub outputs: Option<HashMap<String, String>>,
}

/// `runs-on` can be a single string or a list.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum RunsOn {
    Single(String),
    Multiple(Vec<String>),
}

impl RunsOn {
    pub fn labels(&self) -> Vec<&str> {
        match self {
            RunsOn::Single(s) => vec![s.as_str()],
            RunsOn::Multiple(v) => v.iter().map(|s| s.as_str()).collect(),
        }
    }
}

impl fmt::Display for RunsOn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunsOn::Single(s) => write!(f, "{s}"),
            RunsOn::Multiple(v) => write!(f, "{}", v.join(", ")),
        }
    }
}

/// `needs` can be a single string or a list.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum JobNeeds {
    Single(String),
    Multiple(Vec<String>),
}

impl JobNeeds {
    pub fn as_vec(&self) -> Vec<&str> {
        match self {
            JobNeeds::Single(s) => vec![s.as_str()],
            JobNeeds::Multiple(v) => v.iter().map(|s| s.as_str()).collect(),
        }
    }
}

/// A step in a job.
#[derive(Debug, Deserialize)]
pub struct Step {
    pub id: Option<String>,
    pub name: Option<String>,
    pub run: Option<String>,
    pub uses: Option<String>,
    #[serde(rename = "with")]
    pub with: Option<HashMap<String, serde_json::Value>>,
    pub env: Option<HashMap<String, String>>,
    #[serde(rename = "if")]
    pub condition: Option<String>,
    pub shell: Option<String>,
    #[serde(rename = "working-directory")]
    pub working_directory: Option<String>,
    #[serde(rename = "continue-on-error")]
    pub continue_on_error: Option<bool>,
    #[serde(rename = "timeout-minutes")]
    pub timeout_minutes: Option<u64>,
}

impl Step {
    pub fn display_name(&self) -> String {
        if let Some(name) = &self.name {
            name.clone()
        } else if let Some(run) = &self.run {
            let first_line = run.lines().next().unwrap_or("(empty)");
            if first_line.len() > 60 {
                format!("{}...", &first_line[..57])
            } else {
                first_line.to_string()
            }
        } else if let Some(uses) = &self.uses {
            uses.clone()
        } else {
            "(unnamed step)".to_string()
        }
    }
}

/// Container configuration for a job.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Container {
    Image(String),
    Config(ContainerConfig),
}

impl Container {
    pub fn image(&self) -> &str {
        match self {
            Container::Image(s) => s,
            Container::Config(c) => &c.image,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ContainerConfig {
    pub image: String,
    pub env: Option<HashMap<String, String>>,
    pub ports: Option<Vec<String>>,
    pub volumes: Option<Vec<String>>,
    pub options: Option<String>,
    pub credentials: Option<ContainerCredentials>,
}

#[derive(Debug, Deserialize)]
pub struct ContainerCredentials {
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Service container.
#[derive(Debug, Deserialize)]
pub struct ServiceContainer {
    pub image: String,
    pub env: Option<HashMap<String, String>>,
    pub ports: Option<Vec<String>>,
    pub volumes: Option<Vec<String>>,
    pub options: Option<String>,
    pub credentials: Option<ContainerCredentials>,
}

/// Strategy configuration.
#[derive(Debug, Deserialize)]
pub struct Strategy {
    pub matrix: Option<Matrix>,
    #[serde(rename = "fail-fast")]
    pub fail_fast: Option<bool>,
    #[serde(rename = "max-parallel")]
    pub max_parallel: Option<u32>,
}

/// Matrix configuration.
#[derive(Debug, Deserialize)]
pub struct Matrix {
    pub include: Option<Vec<HashMap<String, serde_json::Value>>>,
    pub exclude: Option<Vec<HashMap<String, serde_json::Value>>>,
    #[serde(flatten)]
    pub dimensions: HashMap<String, serde_json::Value>,
}
