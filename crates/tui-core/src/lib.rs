#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Chat,
    Diff,
    Tasks,
    Agents,
    Status,
    Jobs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    KeyPressed(char),
    PromptSubmitted(String),
    ResponseDelta(String),
    ToolStarted(String),
    ToolFinished(String),
    JobQueued(String),
    JobProgress { job_id: String, progress: u8 },
    JobCompleted(String),
    ApprovalRequested(String),
    ApprovalResolved(String),
    PauseRequested,
    ResumeRequested,
    Tick,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEffect {
    Render,
    PersistCheckpoint,
    ScheduleBackgroundRefresh,
    EmitStatusLine(String),
}

#[derive(Debug, Clone)]
pub struct UiState {
    pub active_pane: Pane,
    pub paused: bool,
    pub last_response_delta: Option<String>,
    pub active_tool: Option<String>,
    pub pending_tasks: usize,
    pub active_jobs: usize,
    pub pending_approvals: usize,
    pub status_line: String,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            active_pane: Pane::Chat,
            paused: false,
            last_response_delta: None,
            active_tool: None,
            pending_tasks: 0,
            active_jobs: 0,
            pending_approvals: 0,
            status_line: "就绪".to_string(),
        }
    }
}

impl UiState {
    pub fn reduce(&mut self, event: UiEvent) -> Vec<UiEffect> {
        match event {
            UiEvent::KeyPressed('1') => {
                self.active_pane = Pane::Chat;
                vec![UiEffect::Render]
            }
            UiEvent::KeyPressed('2') => {
                self.active_pane = Pane::Diff;
                vec![UiEffect::Render]
            }
            UiEvent::KeyPressed('3') => {
                self.active_pane = Pane::Tasks;
                vec![UiEffect::Render]
            }
            UiEvent::KeyPressed('4') => {
                self.active_pane = Pane::Agents;
                vec![UiEffect::Render]
            }
            UiEvent::KeyPressed('5') => {
                self.active_pane = Pane::Jobs;
                vec![UiEffect::Render]
            }
            UiEvent::PromptSubmitted(_) => {
                self.pending_tasks = self.pending_tasks.saturating_add(1);
                self.status_line = "提示已提交".to_string();
                vec![
                    UiEffect::Render,
                    UiEffect::PersistCheckpoint,
                    UiEffect::EmitStatusLine(self.status_line.clone()),
                ]
            }
            UiEvent::ResponseDelta(delta) => {
                self.last_response_delta = Some(delta);
                self.status_line = "正在流式响应".to_string();
                vec![
                    UiEffect::Render,
                    UiEffect::EmitStatusLine(self.status_line.clone()),
                ]
            }
            UiEvent::ToolStarted(name) => {
                self.active_tool = Some(name.clone());
                self.status_line = format!("工具执行中: {name}");
                vec![
                    UiEffect::Render,
                    UiEffect::EmitStatusLine(self.status_line.clone()),
                ]
            }
            UiEvent::ToolFinished(name) => {
                self.active_tool = None;
                self.pending_tasks = self.pending_tasks.saturating_sub(1);
                self.status_line = format!("工具已完成: {name}");
                vec![
                    UiEffect::Render,
                    UiEffect::PersistCheckpoint,
                    UiEffect::EmitStatusLine(self.status_line.clone()),
                ]
            }
            UiEvent::JobQueued(_) => {
                self.active_jobs = self.active_jobs.saturating_add(1);
                self.status_line = "任务已排队".to_string();
                vec![UiEffect::Render, UiEffect::PersistCheckpoint]
            }
            UiEvent::JobProgress { progress, .. } => {
                self.status_line = format!("任务进度: {}%", progress.min(100));
                vec![
                    UiEffect::Render,
                    UiEffect::EmitStatusLine(self.status_line.clone()),
                ]
            }
            UiEvent::JobCompleted(_) => {
                self.active_jobs = self.active_jobs.saturating_sub(1);
                self.status_line = "任务已完成".to_string();
                vec![
                    UiEffect::Render,
                    UiEffect::PersistCheckpoint,
                    UiEffect::EmitStatusLine(self.status_line.clone()),
                ]
            }
            UiEvent::ApprovalRequested(_) => {
                self.pending_approvals = self.pending_approvals.saturating_add(1);
                self.status_line = "请求审批中".to_string();
                vec![
                    UiEffect::Render,
                    UiEffect::EmitStatusLine(self.status_line.clone()),
                ]
            }
            UiEvent::ApprovalResolved(_) => {
                self.pending_approvals = self.pending_approvals.saturating_sub(1);
                self.status_line = "审批已完成".to_string();
                vec![
                    UiEffect::Render,
                    UiEffect::PersistCheckpoint,
                    UiEffect::EmitStatusLine(self.status_line.clone()),
                ]
            }
            UiEvent::PauseRequested => {
                self.paused = true;
                self.status_line = "已暂停".to_string();
                vec![
                    UiEffect::Render,
                    UiEffect::EmitStatusLine(self.status_line.clone()),
                ]
            }
            UiEvent::ResumeRequested => {
                self.paused = false;
                self.status_line = "已恢复".to_string();
                vec![
                    UiEffect::Render,
                    UiEffect::EmitStatusLine(self.status_line.clone()),
                ]
            }
            UiEvent::Tick => vec![UiEffect::ScheduleBackgroundRefresh],
            UiEvent::KeyPressed(_) => Vec::new(),
        }
    }

    pub fn snapshot(&self) -> String {
        format!(
            "pane={:?};paused={};pending_tasks={};active_jobs={};pending_approvals={};active_tool={};status={}",
            self.active_pane,
            self.paused,
            self.pending_tasks,
            self.active_jobs,
            self.pending_approvals,
            self.active_tool.clone().unwrap_or_default(),
            self.status_line
        )
    }
}
