#![allow(deprecated)]

use crate::cloud_provider::io::Kind;
use crate::errors::io::EngineError;
use crate::events;
use serde_derive::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "lowercase")]
pub enum EngineEvent {
    Debug {
        details: EventDetails,
        message: EventMessage,
    },
    Info {
        details: EventDetails,
        message: EventMessage,
    },
    Warning {
        details: EventDetails,
        message: EventMessage,
    },
    Error {
        error: EngineError,
        message: Option<EventMessage>,
    },
}

impl From<events::EngineEvent> for EngineEvent {
    fn from(event: events::EngineEvent) -> Self {
        match event {
            events::EngineEvent::Debug(d, m) => EngineEvent::Debug {
                details: EventDetails::from(d),
                message: EventMessage::from(m),
            },
            events::EngineEvent::Info(d, m) => EngineEvent::Info {
                details: EventDetails::from(d),
                message: EventMessage::from(m),
            },
            events::EngineEvent::Warning(d, m) => EngineEvent::Warning {
                details: EventDetails::from(d),
                message: EventMessage::from(m),
            },
            events::EngineEvent::Error(e, m) => EngineEvent::Error {
                error: EngineError::from(e),
                message: m.map(EventMessage::from),
            },
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub struct EventMessage {
    safe_message: String,
    full_details: Option<String>,
}

impl From<events::EventMessage> for EventMessage {
    fn from(message: events::EventMessage) -> Self {
        EventMessage {
            safe_message: message.safe_message,
            full_details: message.full_details,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    General(GeneralStep),
    Infrastructure(InfrastructureStep),
    Environment(EnvironmentStep),
}

impl From<events::Stage> for Stage {
    fn from(stage: events::Stage) -> Self {
        match stage {
            events::Stage::General(step) => Stage::General(GeneralStep::from(step)),
            events::Stage::Infrastructure(step) => Stage::Infrastructure(InfrastructureStep::from(step)),
            events::Stage::Environment(step) => Stage::Environment(EnvironmentStep::from(step)),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GeneralStep {
    RetrieveClusterConfig,
    RetrieveClusterResources,
    ValidateSystemRequirements,
    UnderMigration,
}

impl From<events::GeneralStep> for GeneralStep {
    fn from(step: events::GeneralStep) -> Self {
        match step {
            events::GeneralStep::RetrieveClusterConfig => GeneralStep::RetrieveClusterConfig,
            events::GeneralStep::RetrieveClusterResources => GeneralStep::RetrieveClusterResources,
            events::GeneralStep::ValidateSystemRequirements => GeneralStep::ValidateSystemRequirements,
            events::GeneralStep::UnderMigration => GeneralStep::UnderMigration,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InfrastructureStep {
    LoadConfiguration,
    Create,
    Pause,
    Resume,
    Downgrade,
    Upgrade,
    Delete,
}

impl From<events::InfrastructureStep> for InfrastructureStep {
    fn from(step: events::InfrastructureStep) -> Self {
        match step {
            events::InfrastructureStep::LoadConfiguration => InfrastructureStep::LoadConfiguration,
            events::InfrastructureStep::Create => InfrastructureStep::Create,
            events::InfrastructureStep::Pause => InfrastructureStep::Pause,
            events::InfrastructureStep::Upgrade => InfrastructureStep::Upgrade,
            events::InfrastructureStep::Delete => InfrastructureStep::Delete,
            events::InfrastructureStep::Resume => InfrastructureStep::Resume,
            events::InfrastructureStep::Downgrade => InfrastructureStep::Downgrade,
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentStep {
    Build,
    Deploy,
    Pause,
    Resume,
    Update,
    Delete,
    LoadConfiguration,
    ScaleUp,
    ScaleDown,
}

impl From<events::EnvironmentStep> for EnvironmentStep {
    fn from(step: events::EnvironmentStep) -> Self {
        match step {
            events::EnvironmentStep::Build => EnvironmentStep::Build,
            events::EnvironmentStep::Deploy => EnvironmentStep::Deploy,
            events::EnvironmentStep::Update => EnvironmentStep::Update,
            events::EnvironmentStep::Delete => EnvironmentStep::Delete,
            events::EnvironmentStep::Pause => EnvironmentStep::Pause,
            events::EnvironmentStep::Resume => EnvironmentStep::Resume,
            events::EnvironmentStep::LoadConfiguration => EnvironmentStep::LoadConfiguration,
            events::EnvironmentStep::ScaleUp => EnvironmentStep::ScaleUp,
            events::EnvironmentStep::ScaleDown => EnvironmentStep::ScaleDown,
        }
    }
}

type TransmitterId = String;
type TransmitterName = String;
type TransmitterType = String;
type TransmitterVersion = String;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type")]
pub enum Transmitter {
    BuildPlatform {
        id: TransmitterId,
        name: TransmitterName,
    },
    ContainerRegistry {
        id: TransmitterId,
        name: TransmitterName,
    },
    CloudProvider {
        id: TransmitterId,
        name: TransmitterName,
    },
    Kubernetes {
        id: TransmitterId,
        name: TransmitterName,
    },
    DnsProvider {
        id: TransmitterId,
        name: TransmitterName,
    },
    ObjectStorage {
        id: TransmitterId,
        name: TransmitterName,
    },
    Environment {
        id: TransmitterId,
        name: TransmitterName,
    },
    Database {
        id: TransmitterId,
        db_type: TransmitterType,
        name: TransmitterName,
    },
    Application {
        id: TransmitterId,
        name: TransmitterName,
        commit: TransmitterVersion,
    },
    Router {
        id: TransmitterId,
        name: TransmitterName,
    },
}

impl From<events::Transmitter> for Transmitter {
    fn from(transmitter: events::Transmitter) -> Self {
        match transmitter {
            events::Transmitter::BuildPlatform(id, name) => Transmitter::BuildPlatform { id, name },
            events::Transmitter::ContainerRegistry(id, name) => Transmitter::ContainerRegistry { id, name },
            events::Transmitter::CloudProvider(id, name) => Transmitter::CloudProvider { id, name },
            events::Transmitter::Kubernetes(id, name) => Transmitter::Kubernetes { id, name },
            events::Transmitter::DnsProvider(id, name) => Transmitter::DnsProvider { id, name },
            events::Transmitter::ObjectStorage(id, name) => Transmitter::ObjectStorage { id, name },
            events::Transmitter::Environment(id, name) => Transmitter::Environment { id, name },
            events::Transmitter::Database(id, db_type, name) => Transmitter::Database { id, db_type, name },
            events::Transmitter::Application(id, name, commit) => Transmitter::Application { id, name, commit },
            events::Transmitter::Router(id, name) => Transmitter::Router { id, name },
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub struct EventDetails {
    provider_kind: Option<Kind>,
    organisation_id: String,
    cluster_id: String,
    execution_id: String,
    region: Option<String>,
    stage: Stage,
    transmitter: Transmitter,
}

impl From<events::EventDetails> for EventDetails {
    fn from(details: events::EventDetails) -> Self {
        let provider_kind = details.provider_kind.map(Kind::from);
        EventDetails {
            provider_kind,
            organisation_id: details.organisation_id.to_string(),
            cluster_id: details.cluster_id.to_string(),
            execution_id: details.execution_id.to_string(),
            region: details.region,
            stage: Stage::from(details.stage),
            transmitter: Transmitter::from(details.transmitter),
        }
    }
}
