use tera::Context as TeraContext;

use crate::cloud_provider::models::{CustomDomain, CustomDomainDataTemplate, Route, RouteDataTemplate};
use crate::cloud_provider::service::{
    default_tera_context, delete_router, deploy_stateless_service_error, send_progress_on_long_task, Action, Create,
    Delete, Helm, Pause, Router as RRouter, Service, ServiceType, StatelessService,
};
use crate::cloud_provider::utilities::{check_cname_for, print_action, sanitize_name};
use crate::cloud_provider::DeploymentTarget;
use crate::cmd::helm::Timeout;
use crate::errors::EngineError;
use crate::events::{EngineEvent, EnvironmentStep, EventMessage, Stage, ToTransmitter, Transmitter};
use crate::logger::{LogLevel, Logger};
use crate::models::{Context, Listen, Listener, Listeners};
use ::function_name::named;

pub struct Router<'a> {
    context: Context,
    id: String,
    name: String,
    action: Action,
    default_domain: String,
    custom_domains: Vec<CustomDomain>,
    sticky_sessions_enabled: bool,
    routes: Vec<Route>,
    listeners: Listeners,
    logger: &'a dyn Logger,
}

impl<'a> Router<'a> {
    pub fn new(
        context: Context,
        id: &str,
        name: &str,
        action: Action,
        default_domain: &str,
        custom_domains: Vec<CustomDomain>,
        routes: Vec<Route>,
        sticky_sessions_enabled: bool,
        listeners: Listeners,
        logger: &'a dyn Logger,
    ) -> Self {
        Router {
            context,
            id: id.to_string(),
            name: name.to_string(),
            action,
            default_domain: default_domain.to_string(),
            custom_domains,
            sticky_sessions_enabled,
            routes,
            listeners,
            logger,
        }
    }

    fn cloud_provider_name(&self) -> &str {
        "aws"
    }

    fn struct_name(&self) -> &str {
        "router"
    }
}

impl<'a> Service for Router<'a> {
    fn context(&self) -> &Context {
        &self.context
    }

    fn service_type(&self) -> ServiceType {
        ServiceType::Router
    }

    fn id(&self) -> &str {
        self.id.as_str()
    }

    fn name(&self) -> &str {
        self.name.as_str()
    }

    fn sanitized_name(&self) -> String {
        sanitize_name("router", self.name())
    }

    fn version(&self) -> String {
        "1.0".to_string()
    }

    fn action(&self) -> &Action {
        &self.action
    }

    fn private_port(&self) -> Option<u16> {
        None
    }

    fn start_timeout(&self) -> Timeout<u32> {
        Timeout::Default
    }

    fn total_cpus(&self) -> String {
        "1".to_string()
    }

    fn cpu_burst(&self) -> String {
        unimplemented!()
    }

    fn total_ram_in_mib(&self) -> u32 {
        1
    }

    fn min_instances(&self) -> u32 {
        1
    }

    fn max_instances(&self) -> u32 {
        1
    }

    fn publicly_accessible(&self) -> bool {
        false
    }

    fn tera_context(&self, target: &DeploymentTarget) -> Result<TeraContext, EngineError> {
        let event_details = self.get_event_details(Stage::Environment(EnvironmentStep::LoadConfiguration));
        let kubernetes = target.kubernetes;
        let environment = target.environment;
        let mut context = default_tera_context(self, kubernetes, environment);

        let applications = environment
            .stateless_services
            .iter()
            .filter(|x| x.service_type() == ServiceType::Application)
            .collect::<Vec<_>>();

        let custom_domain_data_templates = self
            .custom_domains
            .iter()
            .map(|cd| {
                let domain_hash = crate::crypto::to_sha1_truncate_16(cd.domain.as_str());
                CustomDomainDataTemplate {
                    domain: cd.domain.clone(),
                    domain_hash,
                    target_domain: cd.target_domain.clone(),
                }
            })
            .collect::<Vec<_>>();

        let route_data_templates = self
            .routes
            .iter()
            .map(|r| {
                match applications
                    .iter()
                    .find(|app| app.name() == r.application_name.as_str())
                {
                    Some(application) => application.private_port().map(|private_port| RouteDataTemplate {
                        path: r.path.clone(),
                        application_name: application.sanitized_name(),
                        application_port: private_port,
                    }),
                    _ => None,
                }
            })
            .filter(|x| x.is_some())
            .map(|x| x.unwrap())
            .collect::<Vec<_>>();

        // autoscaler
        context.insert("nginx_enable_horizontal_autoscaler", "false");
        context.insert("nginx_minimum_replicas", "1");
        context.insert("nginx_maximum_replicas", "10");
        // resources
        context.insert("nginx_requests_cpu", "200m");
        context.insert("nginx_requests_memory", "128Mi");
        context.insert("nginx_limit_cpu", "200m");
        context.insert("nginx_limit_memory", "128Mi");

        let kubernetes_config_file_path = kubernetes.get_kubeconfig_file_path()?;

        // Default domain
        match crate::cmd::kubectl::kubectl_exec_get_external_ingress_hostname(
            kubernetes_config_file_path,
            "nginx-ingress",
            "nginx-ingress-ingress-nginx-controller",
            kubernetes.cloud_provider().credentials_environment_variables(),
        ) {
            Ok(external_ingress_hostname_default) => match external_ingress_hostname_default {
                Some(hostname) => context.insert("external_ingress_hostname_default", hostname.as_str()),
                None => {
                    // TODO(benjaminch): Handle better this one via a proper error eventually
                    self.logger().log(LogLevel::Warning, EngineEvent::Warning(event_details.clone(), EventMessage::new_from_safe("unable to get external_ingress_hostname_default - what's wrong? This must never happened".to_string())));
                }
            },
            _ => {
                // FIXME really?
                // TODO(benjaminch): Handle better this one via a proper error eventually
                self.logger().log(
                    LogLevel::Warning,
                    EngineEvent::Warning(
                        event_details.clone(),
                        EventMessage::new_from_safe(
                            "can't fetch kubernetes config file - what's wrong? This must never happened".to_string(),
                        ),
                    ),
                );
            }
        }

        let router_default_domain_hash = crate::crypto::to_sha1_truncate_16(self.default_domain.as_str());

        let tls_domain = format!("*.{}", kubernetes.dns_provider().domain());
        context.insert("router_tls_domain", tls_domain.as_str());
        context.insert("router_default_domain", self.default_domain.as_str());
        context.insert("router_default_domain_hash", router_default_domain_hash.as_str());
        context.insert("custom_domains", &custom_domain_data_templates);
        context.insert("routes", &route_data_templates);
        context.insert("spec_acme_email", "tls@qovery.com"); // TODO CHANGE ME
        context.insert("metadata_annotations_cert_manager_cluster_issuer", "letsencrypt-qovery");

        let lets_encrypt_url = match self.context.is_test_cluster() {
            true => "https://acme-staging-v02.api.letsencrypt.org/directory",
            false => "https://acme-v02.api.letsencrypt.org/directory",
        };
        context.insert("spec_acme_server", lets_encrypt_url);

        // Nginx
        context.insert("sticky_sessions_enabled", &self.sticky_sessions_enabled);

        Ok(context)
    }

    fn logger(&self) -> &dyn Logger {
        self.logger
    }

    fn selector(&self) -> Option<String> {
        Some(format!("routerId={}", self.id))
    }
}

impl<'a> crate::cloud_provider::service::Router for Router<'a> {
    fn domains(&self) -> Vec<&str> {
        let mut _domains = vec![self.default_domain.as_str()];

        for domain in &self.custom_domains {
            _domains.push(domain.domain.as_str());
        }

        _domains
    }

    fn has_custom_domains(&self) -> bool {
        !self.custom_domains.is_empty()
    }
}

impl<'a> Helm for Router<'a> {
    fn helm_selector(&self) -> Option<String> {
        self.selector()
    }

    fn helm_release_name(&self) -> String {
        crate::string::cut(format!("router-{}", self.id()), 50)
    }

    fn helm_chart_dir(&self) -> String {
        format!("{}/common/charts/ingress-nginx", self.context().lib_root_dir())
    }

    fn helm_chart_values_dir(&self) -> String {
        format!("{}/aws/chart_values/nginx-ingress", self.context.lib_root_dir())
    }

    fn helm_chart_external_name_service_dir(&self) -> String {
        String::new()
    }
}

impl<'a> Listen for Router<'a> {
    fn listeners(&self) -> &Listeners {
        &self.listeners
    }

    fn add_listener(&mut self, listener: Listener) {
        self.listeners.push(listener);
    }
}

impl<'a> StatelessService for Router<'a> {}

impl<'a> ToTransmitter for Router<'a> {
    fn to_transmitter(&self) -> Transmitter {
        Transmitter::Router(self.id().to_string(), self.name().to_string())
    }
}

impl<'a> Create for Router<'a> {
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
        let kubernetes = target.kubernetes;
        let environment = target.environment;
        let workspace_dir = self.workspace_directory();
        let helm_release_name = self.helm_release_name();

        let kubernetes_config_file_path = kubernetes.get_kubeconfig_file_path()?;

        // respect order - getting the context here and not before is mandatory
        // the nginx-ingress must be available to get the external dns target if necessary
        let context = self.tera_context(target)?;

        let from_dir = format!("{}/aws/charts/q-ingress-tls", self.context.lib_root_dir());
        if let Err(e) =
            crate::template::generate_and_copy_all_files_into_dir(from_dir.as_str(), workspace_dir.as_str(), context)
        {
            return Err(EngineError::new_cannot_copy_files_from_one_directory_to_another(
                event_details.clone(),
                from_dir.to_string(),
                workspace_dir.to_string(),
                e,
            ));
        }

        // do exec helm upgrade and return the last deployment status
        let helm_history_row = crate::cmd::helm::helm_exec_with_upgrade_history(
            kubernetes_config_file_path.as_str(),
            environment.namespace(),
            helm_release_name.as_str(),
            self.selector(),
            workspace_dir.as_str(),
            self.start_timeout(),
            kubernetes.cloud_provider().credentials_environment_variables(),
            self.service_type(),
        )
        .map_err(|e| EngineError::new_helm_charts_upgrade_error(event_details.clone(), e))?;

        if helm_history_row.is_none() || !helm_history_row.unwrap().is_successfully_deployed() {
            return Err(EngineError::new_router_failed_to_deploy(event_details.clone()));
        }

        Ok(())
    }

    fn on_create_check(&self) -> Result<(), EngineError> {
        let event_details = self.get_event_details(Stage::Environment(EnvironmentStep::Deploy));

        // check non custom domains
        self.check_domains()?;

        // Wait/Check that custom domain is a CNAME targeting qovery
        for domain_to_check in self.custom_domains.iter() {
            match check_cname_for(
                self.progress_scope(),
                self.listeners(),
                &domain_to_check.domain,
                self.context.execution_id(),
            ) {
                Ok(cname) if cname.trim_end_matches('.') == domain_to_check.target_domain.trim_end_matches('.') => {
                    continue
                }
                Ok(err) | Err(err) => {
                    // TODO(benjaminch): Handle better this one via a proper error eventually
                    self.logger().log(
                        LogLevel::Warning,
                        EngineEvent::Warning(
                            event_details.clone(),
                            EventMessage::new(
                                format!(
                                    "Invalid CNAME for {}. Might not be an issue if user is using a CDN.",
                                    domain_to_check.domain,
                                ),
                                Some(err.to_string()),
                            ),
                        ),
                    );
                }
            }
        }

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
            event_details,
            self.logger(),
        );

        send_progress_on_long_task(self, crate::cloud_provider::service::Action::Create, || {
            deploy_stateless_service_error(target, self)
        })
    }
}

impl<'a> Pause for Router<'a> {
    #[named]
    fn on_pause(&self, _target: &DeploymentTarget) -> Result<(), EngineError> {
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

impl<'a> Delete for Router<'a> {
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
        delete_router(target, self, false, event_details)
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
        delete_router(target, self, true, event_details)
    }
}
