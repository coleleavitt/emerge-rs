// phases.rs - Ebuild phase execution
use std::collections::HashMap;
use crate::exception::InvalidData;
use super::environment::EbuildEnvironment;

/// Ebuild phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    Setup,
    Unpack,
    Prepare,
    Configure,
    Compile,
    Test,
    Install,
    PreInst,
    PostInst,
    PreRm,
    PostRm,
}

impl Phase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Phase::Setup => "pkg_setup",
            Phase::Unpack => "src_unpack",
            Phase::Prepare => "src_prepare",
            Phase::Configure => "src_configure",
            Phase::Compile => "src_compile",
            Phase::Test => "src_test",
            Phase::Install => "src_install",
            Phase::PreInst => "pkg_preinst",
            Phase::PostInst => "pkg_postinst",
            Phase::PreRm => "pkg_prerm",
            Phase::PostRm => "pkg_postrm",
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pkg_setup" => Some(Phase::Setup),
            "src_unpack" => Some(Phase::Unpack),
            "src_prepare" => Some(Phase::Prepare),
            "src_configure" => Some(Phase::Configure),
            "src_compile" => Some(Phase::Compile),
            "src_test" => Some(Phase::Test),
            "src_install" => Some(Phase::Install),
            "pkg_preinst" => Some(Phase::PreInst),
            "pkg_postinst" => Some(Phase::PostInst),
            "pkg_prerm" => Some(Phase::PreRm),
            "pkg_postrm" => Some(Phase::PostRm),
            _ => None,
        }
    }
}

/// Phase function type
pub type PhaseFunction = fn(&mut EbuildEnvironment) -> Result<(), InvalidData>;

/// Phase executor
pub struct PhaseExecutor {
    phases: HashMap<Phase, PhaseFunction>,
}

impl PhaseExecutor {
    pub fn new() -> Self {
        PhaseExecutor {
            phases: HashMap::new(),
        }
    }
    
    /// Register a phase function
    pub fn register(&mut self, phase: Phase, func: PhaseFunction) {
        self.phases.insert(phase, func);
    }
    
    /// Execute a phase
    pub fn execute(&self, phase: Phase, env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        if let Some(func) = self.phases.get(&phase) {
            func(env)
        } else {
            // Run default phase if no custom implementation
            self.default_phase(phase, env)
        }
    }
    
    /// Default phase implementation
    fn default_phase(&self, phase: Phase, env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        match phase {
            Phase::Setup => Ok(()),
            Phase::Unpack => self.default_unpack(env),
            Phase::Prepare => Ok(()),
            Phase::Configure => self.default_configure(env),
            Phase::Compile => self.default_compile(env),
            Phase::Test => Ok(()),
            Phase::Install => self.default_install(env),
            Phase::PreInst | Phase::PostInst | Phase::PreRm | Phase::PostRm => Ok(()),
        }
    }
    
    fn default_unpack(&self, env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        use super::helpers::default_src_unpack;
        default_src_unpack(env)
    }
    
    fn default_configure(&self, env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        use super::helpers::default_src_configure;
        default_src_configure(env)
    }
    
    fn default_compile(&self, env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        use super::helpers::emake;
        emake(env, &[])?;
        Ok(())
    }
    
    fn default_install(&self, env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        use super::helpers::emake;
        let destdir = env.destdir.to_string_lossy().to_string();
        let output = emake(env, &["install", &format!("DESTDIR={}", destdir)])?;
        
        if !output.status.success() {
            return Err(InvalidData::new("make install failed", None));
        }
        
        Ok(())
    }
}

impl Default for PhaseExecutor {
    fn default() -> Self {
        Self::new()
    }
}
