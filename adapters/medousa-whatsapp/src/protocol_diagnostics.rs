//! Opt-in, allowlisted protocol stages. Never output upstream log arguments:
//! those can contain account identifiers, stanza IDs, or message contents.
struct ProtocolLog;
static LOGGER: ProtocolLog = ProtocolLog;
impl log::Log for ProtocolLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.target() == "whatsapp_rust::message::msg_secret"
    }
    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let message = record.args().to_string();
        let stage = if message.contains("Successfully decrypted msmsg") {
            "msmsg-decrypted"
        } else if message.contains("msmsg: no message_secret") {
            "msmsg-missing-secret"
        } else if message.contains("msmsg AES-GCM") {
            "msmsg-authentication-failed"
        } else if message.contains("msmsg plaintext is not") {
            "msmsg-invalid-protobuf"
        } else if message.contains("MessageSecretMessage") {
            "msmsg-invalid-envelope"
        } else if message.contains("msmsg: no target_sender") {
            "msmsg-missing-target-sender"
        } else if message.contains("msmsg: <meta> missing") {
            "msmsg-missing-target-id"
        } else {
            return;
        };
        eprintln!("medousa_whatsapp protocol: {stage}");
    }
    fn flush(&self) {}
}
pub fn enabled() -> bool {
    std::env::var("MEDOUSA_WHATSAPP_PROTOCOL_DIAGNOSTICS").is_ok_and(|v| v == "1")
}
pub fn init() {
    if enabled() && log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}
