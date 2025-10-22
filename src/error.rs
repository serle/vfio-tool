use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum VfioError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("IOMMU not enabled. Run 'vfio-tool setup-grub' to enable it.")]
    IommuNotEnabled,

    #[error("VFIO module not loaded. Run 'sudo modprobe vfio-pci'")]
    VfioModuleNotLoaded,

    #[error("Device {0} is already bound to {1}")]
    AlreadyBound(String, String),

    #[error("Device {0} is in use and cannot be unbound")]
    DeviceInUse(String),

    #[error("IOMMU group {0} contains multiple devices. All must be bound together.")]
    MultiDeviceGroup(u32),

    #[error("Permission denied. Try running with sudo.")]
    PermissionDenied,

    #[error("Configuration file not found")]
    ConfigNotFound,

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("GRUB configuration error: {0}")]
    GrubError(String),

    #[error("Systemd error: {0}")]
    SystemdError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[allow(dead_code)]
pub type Result<T> = std::result::Result<T, VfioError>;
