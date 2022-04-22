locals {
  kubeconfig_base64 = base64encode(local.kubeconfig)
}
// do not run for tests clusters to avoid uncleaned info.
// do not try to use count into resource, it will fails trying to connect to vault
{% if vault_auth_method != "none" and not test_cluster %}
resource "vault_generic_secret" "cluster-access" {
  path = "official-clusters-access/${var.organization_id}-${var.kubernetes_cluster_id}"

  data_json = <<EOT
{
  "cloud_provider": "${var.cloud_provider}",
  "cluster_name": "${var.kubernetes_cluster_name}",
  "KUBECONFIG_b64": "${local.kubeconfig_base64}",
  "organization_id": "${var.organization_id}",
  "test_cluster": "${var.test_cluster}",
  "grafana_login": "{{ grafana_admin_user }}",
  "grafana_password": "{{ grafana_admin_password }}",
  "AWS_ACCESS_KEY_ID": "{{ aws_access_key }}",
  "AWS_SECRET_ACCESS_KEY": "{{ aws_secret_key }}",
  "AWS_DEFAULT_REGION": "{{ aws_region }}"
}
EOT

  depends_on = [
    aws_eks_cluster.eks_cluster,
  ]
}
{% endif %}