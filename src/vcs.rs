use gix::ThreadSafeRepository;
use std::path::Path;

pub trait VcsBackend {
    fn get_status_text(&self) -> Result<String, String>;
    fn execute_mutation(&self, command: &str, args: Vec<&str>) -> Result<(), String>;
}

pub struct GitoxideBackend {
    repo: ThreadSafeRepository,
}

impl GitoxideBackend {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let repo = gix::discover(path).map_err(|e| e.to_string())?;
        Ok(Self {
            repo: repo.into_sync(),
        })
    }
}

impl VcsBackend for GitoxideBackend {
    fn get_status_text(&self) -> Result<String, String> {
        let _repo = self.repo.to_thread_local();
        Ok("Head: main\n\nUnstaged changes:\n- modified file.rs\n".to_string())
    }

    fn execute_mutation(&self, _command: &str, _args: Vec<&str>) -> Result<(), String> {
        Ok(())
    }
}