use serde::{Deserialize, Serialize};

pub const JOB_LEDGER_SCHEMA_V1: &str = "aethra.job-ledger.v1";

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum JobError {
    #[error("Job objective is required")]
    MissingObjective,
    #[error("Terminal jobs cannot be modified")]
    Terminal,
    #[error("Trigger time must be in the future")]
    PastTrigger,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Scheduled,
    Active,
    Blocked,
    Completed,
    Cancelled,
    Unknown,
}

impl JobStatus {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Unknown)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobTrigger {
    pub run_at: Option<u64>,
    pub condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DurableJob {
    pub schema: String,
    pub id: String,
    pub task_id: Option<String>,
    pub objective: String,
    pub success_condition: String,
    pub status: JobStatus,
    pub trigger: JobTrigger,
    pub allowed_tools: Vec<String>,
    pub max_actions: u32,
    pub actions_taken: u32,
    pub created_at: u64,
    pub updated_at: u64,
}

impl DurableJob {
    pub fn validate(&self) -> Result<(), JobError> {
        if self.schema != JOB_LEDGER_SCHEMA_V1 {
            return Err(JobError::MissingObjective);
        }
        if self.objective.trim().is_empty() {
            return Err(JobError::MissingObjective);
        }
        Ok(())
    }

    pub fn schedule(
        id: &str,
        objective: &str,
        run_at: u64,
        now_millis: u64,
    ) -> Result<Self, JobError> {
        if objective.trim().is_empty() {
            return Err(JobError::MissingObjective);
        }
        if run_at <= now_millis {
            return Err(JobError::PastTrigger);
        }
        Ok(Self {
            schema: JOB_LEDGER_SCHEMA_V1.into(),
            id: id.into(),
            task_id: None,
            objective: objective.into(),
            success_condition: String::new(),
            status: JobStatus::Scheduled,
            trigger: JobTrigger {
                run_at: Some(run_at),
                condition: None,
            },
            allowed_tools: vec![],
            max_actions: 10,
            actions_taken: 0,
            created_at: now_millis,
            updated_at: now_millis,
        })
    }

    pub fn cancel(&mut self, now_millis: u64) -> Result<(), JobError> {
        if self.status.terminal() {
            return Err(JobError::Terminal);
        }
        self.status = JobStatus::Cancelled;
        self.updated_at = now_millis;
        Ok(())
    }

    pub fn record_action(&mut self, now_millis: u64) -> Result<(), JobError> {
        if self.status.terminal() {
            return Err(JobError::Terminal);
        }
        self.actions_taken += 1;
        self.updated_at = now_millis;
        Ok(())
    }

    pub fn due(&self, now_millis: u64) -> bool {
        self.status == JobStatus::Scheduled
            && self.trigger.run_at.is_some_and(|at| at <= now_millis)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_creates_scheduled_job() {
        let job = DurableJob::schedule("job-1", "remind me", 2000, 1000).unwrap();
        assert_eq!(job.status, JobStatus::Scheduled);
        assert!(job.due(2000));
        assert!(!job.due(1999));
    }

    #[test]
    fn past_trigger_rejected() {
        assert_eq!(
            DurableJob::schedule("job-1", "remind me", 500, 1000),
            Err(JobError::PastTrigger)
        );
    }

    #[test]
    fn empty_objective_rejected() {
        assert_eq!(
            DurableJob::schedule("job-1", "  ", 2000, 1000),
            Err(JobError::MissingObjective)
        );
    }

    #[test]
    fn cancel_transitions() {
        let mut job = DurableJob::schedule("job-1", "remind me", 2000, 1000).unwrap();
        job.cancel(1500).unwrap();
        assert_eq!(job.status, JobStatus::Cancelled);
        assert_eq!(job.cancel(1500), Err(JobError::Terminal));
    }

    #[test]
    fn terminal_jobs_reject_actions() {
        let mut job = DurableJob::schedule("job-1", "remind me", 2000, 1000).unwrap();
        job.cancel(1500).unwrap();
        assert_eq!(job.record_action(1600), Err(JobError::Terminal));
    }

    #[test]
    fn action_budget_counts() {
        let mut job = DurableJob::schedule("job-1", "remind me", 2000, 1000).unwrap();
        job.record_action(1500).unwrap();
        assert_eq!(job.actions_taken, 1);
    }

    #[test]
    fn job_roundtrip() {
        let job = DurableJob::schedule("job-1", "remind me", 2000, 1000).unwrap();
        let json = serde_json::to_string(&job).unwrap();
        let decoded: DurableJob = serde_json::from_str(&json).unwrap();
        assert_eq!(job, decoded);
    }
}
