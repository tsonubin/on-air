use crate::sender::{AudioSender, SenderError};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothDevice {
    pub id: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
}

pub trait BluetoothAdapter: Send + Sync {
    fn list(&self) -> Vec<BluetoothDevice>;
    fn pair(&self, id: &str) -> Result<(), SenderError>;
    fn connect(&self, id: &str) -> Result<(), SenderError>;
    fn disconnect(&self, id: &str) -> Result<(), SenderError>;
    fn set_volume(&self, id: &str, volume: u8) -> Result<(), SenderError>;
}

#[derive(Default)]
pub struct MockBluetoothAdapter {
    devices: Mutex<Vec<BluetoothDevice>>,
}

impl MockBluetoothAdapter {
    pub fn with_devices(devices: Vec<BluetoothDevice>) -> Self {
        MockBluetoothAdapter {
            devices: Mutex::new(devices),
        }
    }
}

impl BluetoothAdapter for MockBluetoothAdapter {
    fn list(&self) -> Vec<BluetoothDevice> {
        self.devices.lock().unwrap().clone()
    }

    fn pair(&self, id: &str) -> Result<(), SenderError> {
        let mut devices = self.devices.lock().unwrap();
        let device = devices
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or_else(|| SenderError("bluetooth device not found".into()))?;
        device.paired = true;
        Ok(())
    }

    fn connect(&self, id: &str) -> Result<(), SenderError> {
        let mut devices = self.devices.lock().unwrap();
        let device = devices
            .iter_mut()
            .find(|d| d.id == id)
            .ok_or_else(|| SenderError("bluetooth device not found".into()))?;
        if !device.paired {
            return Err(SenderError("bluetooth device not paired".into()));
        }
        for d in devices.iter_mut() {
            d.connected = false;
        }
        devices
            .iter_mut()
            .find(|d| d.id == id)
            .unwrap()
            .connected = true;
        Ok(())
    }

    fn disconnect(&self, id: &str) -> Result<(), SenderError> {
        let mut devices = self.devices.lock().unwrap();
        if let Some(device) = devices.iter_mut().find(|d| d.id == id) {
            device.connected = false;
        }
        Ok(())
    }

    fn set_volume(&self, _id: &str, _volume: u8) -> Result<(), SenderError> {
        Ok(())
    }
}

pub struct BluetoothSender {
    id: String,
    name: String,
    adapter: Arc<dyn BluetoothAdapter>,
}

impl BluetoothSender {
    pub fn new(id: impl Into<String>, name: impl Into<String>, adapter: Arc<dyn BluetoothAdapter>) -> Self {
        BluetoothSender {
            id: id.into(),
            name: name.into(),
            adapter,
        }
    }
}

#[async_trait::async_trait]
impl AudioSender for BluetoothSender {
    async fn start(&mut self) -> Result<(), SenderError> {
        self.adapter.connect(&self.id)
    }

    async fn stop(&mut self) -> Result<(), SenderError> {
        self.adapter.disconnect(&self.id)
    }

    async fn set_volume(&mut self, volume: u8) -> Result<(), SenderError> {
        self.adapter.set_volume(&self.id, volume)
    }

    fn name(&self) -> &str {
        &self.name
    }
}
