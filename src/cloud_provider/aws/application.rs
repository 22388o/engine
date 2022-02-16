use tera::Context as TeraContext;

use crate::build_platform::Image;
use crate::cloud_provider::kubernetes::validate_k8s_required_cpu_and_burstable;
use crate::cloud_provider::models::{
    EnvironmentVariable, EnvironmentVariableDataTemplate, Storage, StorageDataTemplate,
};
use crate::cloud_provider::service::{
    default_tera_context, delete_stateless_service, deploy_stateless_service_error, deploy_user_stateless_service,
    scale_down_application, send_progress_on_long_task, Action, Application as CApplication, Create, Delete, Helm,
    Pause, Service, ServiceType, StatelessService,
};
use crate::cloud_provider::utilities::{print_action, sanitize_name};
use crate::cloud_provider::DeploymentTarget;
use crate::cmd::helm::Timeout;
use crate::cmd::kubectl::ScalingKind::{Deployment, Statefulset};
use crate::errors::EngineError;
use crate::events::{EngineEvent, EnvironmentStep, EventMessage, Stage, ToTransmitter, Transmitter};
use crate::logger::{LogLevel, Logger};
use crate::models::{Context, Listen, Listener, Listeners, ListenersHelper, Port};
use ::function_name::named;

pub struct Application<'a> {
    context: Context,
    id: String,
    action: Action,
    name: String,
    ports: Vec<Port>,
    total_cpus: String,
    cpu_burst: String,
    total_ram_in_mib: u32,
    min_instances: u32,
    max_instances: u32,
    start_timeout_in_seconds: u32,
    image: Image,
    storage: Vec<Storage<StorageType>>,
    environment_variables: Vec<EnvironmentVariable>,
    listeners: Listeners,
    logger: &'a dyn Logger,
}

impl<'a> Application<'a> {
    pub fn new(
        context: Context,
        id: &str,
        action: Action,
        name: &str,
        ports: Vec<Port>,
        total_cpus: String,
        cpu_burst: String,
        total_ram_in_mib: u32,
        min_instances: u32,
        max_instances: u32,
        start_timeout_in_seconds: u32,
        image: Image,
        storage: Vec<Storage<StorageType>>,
        environment_variables: Vec<EnvironmentVariable>,
        listeners: Listeners,
        logger: &dyn Logger,
    ) -> Self {
        Application {
            context,
            id: id.to_string(),
            action,
            name: name.to_string(),
            ports,
            total_cpus,
            cpu_burst,
            total_ram_in_mib,
            min_instances,
            max_instances,
            start_timeout_in_seconds,
            image,
            storage,
            environment_variables,
            listeners,
            logger,
        }
    }

    fn is_stateful(&self) -> bool {
        !self.storage.is_empty()
    }

    fn cloud_provider_name(&self) -> &str {
        "aws"
    }

    fn struct_name(&self) -> &str {
        "application"
    }
}

impl<'a> crate::cloud_provider::service::Application<'a> for Application<'a> {
    fn image(&self) -> &Image {
        &self.image
    }

    fn set_image(&mut self, image: Image) {
        self.image = image;
    }
}

impl<'a> Helm for Application<'a> {
    fn helm_selector(&self) -> Option<String> {
        self.selector()
    }

    fn helm_release_name(&self) -> String {
        crate::string::cut(format!("application-{}-{}", self.name(), self.id()), 50)
    }

    fn helm_chart_dir(&self) -> String {
        format!("{}/aws/charts/q-application", self.context.lib_root_dir())
    }

    fn helm_chart_values_dir(&self) -> String {
        String::new()
    }

    fn helm_chart_external_name_service_dir(&self) -> String {
        String::new()
    }
}

impl<'a> StatelessService<'a> for Application<'a> {}

impl<'a> ToTransmitter for Application<'a> {
    fn to_transmitter(&self) -> Transmitter {
        Transmitter::Application(self.id.to_string(), self.name.to_string())
    }
}

impl<'a> Service for Application<'a> {
    fn context(&self) -> &Context {
        &self.context
    }

    fn service_type(&self) -> ServiceType {
        ServiceType::Application
    }

    fn id(&self) -> &str {
        self.id.as_str()
    }

    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn sanitized_name(&self) -> String {
        sanitize_name("app", self.name())
    }

    fn version(&self) -> String {
        self.image.commit_id.clone()
    }

    fn action(&self) -> &Action {
        &self.action
    }

    fn private_port(&self) -> Option<u16> {
        self.ports
            .iter()
            .find(|port| port.publicly_accessible)
            .map(|port| port.port)
    }

    fn start_timeout(&self) -> Timeout<u32> {
        Timeout::Value((self.start_timeout_in_seconds + 10) * 4)
    }

    fn total_cpus(&self) -> String {
        self.total_cpus.to_string()
    }

    fn cpu_burst(&self) -> String {
        self.cpu_burst.to_string()
    }

    fn total_ram_in_mib(&self) -> u32 {
        self.total_ram_in_mib
    }

    fn min_instances(&self) -> u32 {
        self.min_instances
    }

    fn max_instances(&self) -> u32 {
        self.max_instances
    }

    fn publicly_accessible(&self) -> bool {
        self.private_port().is_some()
    }

    fn tera_context(&self, target: &DeploymentTarget) -> Result<TeraContext, EngineError> {
        let event_details = self.get_event_details(Stage::Environment(EnvironmentStep::LoadConfiguration));
        let mut context = default_tera_context(self, target.kubernetes, target.environment);
        let commit_id = self.image().commit_id.as_str();

        context.insert("helm_app_version", &commit_id[..7]);

        match &self.image().registry_url {
            Some(registry_url) => context.insert("image_name_with_tag", registry_url.as_str()),
            None => {
                let image_name_with_tag = self.image().name_with_tag();

                self.logger().log(
                    LogLevel::Warning,
                    EngineEvent::Warning(
                        event_details.clone(),
                        EventMessage::new_from_safe(format!(
                            "there is no registry url, use image name with tag with the default container registry: {}",
                            image_name_with_tag.as_str()
                        )),
                    ),
                );

                context.insert("image_name_with_tag", image_name_with_tag.as_str());
            }
        }

        let environment_variables = self
            .environment_variables
            .iter()
            .map(|ev| EnvironmentVariableDataTemplate {
                key: ev.key.clone(),
                value: ev.value.clone(),
            })
            .collect::<Vec<_>>();

        context.insert("environment_variables", &environment_variables);
        context.insert("ports", &self.ports);

        match self.image.registry_name.as_ref() {
            Some(registry_name) => {
                context.insert("is_registry_secret", &true);
                context.insert("registry_secret", registry_name);
            }
            None => {
                context.insert("is_registry_secret", &false);
            }
        };

        let cpu_limits = match validate_k8s_required_cpu_and_burstable(
            &ListenersHelper::new(&self.listeners),
            &self.context.execution_id(),
            &self.id,
            self.total_cpus(),
            self.cpu_burst(),
            event_details.clone(),
            self.logger(),
        ) {
            Ok(l) => l,
            Err(e) => {
                return Err(EngineError::new_k8s_validate_required_cpu_and_burstable_error(
                    event_details.clone(),
                    self.total_cpus(),
                    self.cpu_burst(),
                    e,
                ));
            }
        };

        context.insert("cpu_burst", &cpu_limits.cpu_limit);

        let storage = self
            .storage
            .iter()
            .map(|s| StorageDataTemplate {
                id: s.id.clone(),
                name: s.name.clone(),
                storage_type: match s.storage_type {
                    StorageType::SC1 => "sc1",
                    StorageType::ST1 => "st1",
                    StorageType::GP2 => "gp2",
                    StorageType::IO1 => "io1",
                }
                .to_string(),
                size_in_gib: s.size_in_gib,
                mount_point: s.mount_point.clone(),
                snapshot_retention_in_days: s.snapshot_retention_in_days,
            })
            .collect::<Vec<_>>();

        let is_storage = !storage.is_empty();

        context.insert("storage", &storage);
        context.insert("is_storage", &is_storage);
        context.insert("clone", &false);
        context.insert("start_timeout_in_seconds", &self.start_timeout_in_seconds);

        if self.context.resource_expiration_in_seconds().is_some() {
            context.insert(
                "resource_expiration_in_seconds",
                &self.context.resource_expiration_in_seconds(),
            )
        }

        Ok(context)
    }

    fn logger(&self) -> &dyn Logger {
        todo!()
    }

    fn selector(&self) -> Option<String> {
        Some(format!("appId={}", self.id))
    }
}

impl<'a> Create for Application<'a> {
    #[named]
    fn on_create(&self, target: &DeploymentTarget) -> Result<(), EngineError> {
        let event_details = self.get_event_details(Stage::Environment(EnvironmentStep::Deploy));
        print_action(
            self.cloud_provider_name(),
            self.struct_name(),
            function_name!(),
            self.name(),
            event_details.clone(),
            self.logger(),
        );
        send_progress_on_long_task(self, crate::cloud_provider::service::Action::Create, || {
            deploy_user_stateless_service(target, self, event_details)
        })
    }

    fn on_create_check(&self) -> Result<(), EngineError> {
        Ok(())
    }

    #[named]
    fn on_create_error(&self, target: &DeploymentTarget) -> Result<(), EngineError> {
        let event_details = self.get_event_details(Stage::Environment(EnvironmentStep::Deploy));
        print_action(
            self.cloud_provider_name(),
            self.struct_name(),
            function_name!(),
            self.name(),
            event_details.clone(),
            self.logger(),
        );

        send_progress_on_long_task(self, crate::cloud_provider::service::Action::Create, || {
            deploy_stateless_service_error(target, self)
        })
    }
}

impl<'a> Pause for Application<'a> {
    #[named]
    fn on_pause(&self, target: &DeploymentTarget) -> Result<(), EngineError> {
        let event_details = self.get_event_details(Stage::Environment(EnvironmentStep::Pause));
        print_action(
            self.cloud_provider_name(),
            self.struct_name(),
            function_name!(),
            self.name(),
            event_details,
            self.logger(),
        );

        send_progress_on_long_task(self, crate::cloud_provider::service::Action::Pause, || {
            scale_down_application(
                target,
                self,
                0,
                if self.is_stateful() { Statefulset } else { Deployment },
            )
        })
    }

    fn on_pause_check(&self) -> Result<(), EngineError> {
        Ok(())
    }

    #[named]
    fn on_pause_error(&self, _target: &DeploymentTarget) -> Result<(), EngineError> {
        let event_details = self.get_event_details(Stage::Environment(EnvironmentStep::Pause));
        print_action(
            self.cloud_provider_name(),
            self.struct_name(),
            function_name!(),
            self.name(),
            event_details,
            self.logger(),
        );

        Ok(())
    }
}

impl<'a> Delete for Application<'a> {
    #[named]
    fn on_delete(&self, target: &DeploymentTarget) -> Result<(), EngineError> {
        let event_details = self.get_event_details(Stage::Environment(EnvironmentStep::Delete));
        print_action(
            self.cloud_provider_name(),
            self.struct_name(),
            function_name!(),
            self.name(),
            event_details.clone(),
            self.logger(),
        );

        send_progress_on_long_task(self, crate::cloud_provider::service::Action::Delete, || {
            delete_stateless_service(target, self, false, event_details)
        })
    }

    fn on_delete_check(&self) -> Result<(), EngineError> {
        Ok(())
    }

    #[named]
    fn on_delete_error(&self, target: &DeploymentTarget) -> Result<(), EngineError> {
        let event_details = self.get_event_details(Stage::Environment(EnvironmentStep::Delete));
        print_action(
            self.cloud_provider_name(),
            self.struct_name(),
            function_name!(),
            self.name(),
            event_details.clone(),
            self.logger(),
        );

        send_progress_on_long_task(self, crate::cloud_provider::service::Action::Delete, || {
            delete_stateless_service(target, self, true, event_details)
        })
    }
}

impl<'a> Listen for Application<'a> {
    fn listeners(&self) -> &Listeners {
        &self.listeners
    }

    fn add_listener(&mut self, listener: Listener) {
        self.listeners.push(listener);
    }
}

#[derive(Clone, Eq, PartialEq, Hash)]
pub enum StorageType {
    SC1,
    ST1,
    GP2,
    IO1,
}
