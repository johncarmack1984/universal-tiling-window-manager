# Lets the release workflow read its signing material from Secrets Manager
# through GitHub's OIDC, so no AWS keys live in GitHub. The secrets themselves
# are created by hand, never here:
#
# - universal-tiling-window-manager/updater-signing-key: the updater's
#   minisign key as JSON {key, password}. Every installed copy installs what
#   it signs.
# - lux/apple-signing: the Developer ID certificate that signs the app and the
#   App Store Connect key that notarizes it, as JSON {certificate,
#   certificate_password, signing_identity, team_id, api_key, api_key_id,
#   api_issuer_id}. Both belong to the Apple account rather than to one app;
#   lux keeps them, and this reads that copy instead of keeping a second one
#   to rotate.

data "aws_iam_openid_connect_provider" "github" {
  url = "https://token.actions.githubusercontent.com"
}

data "aws_secretsmanager_secret" "updater_signing_key" {
  name = "universal-tiling-window-manager/updater-signing-key"
}

data "aws_secretsmanager_secret" "apple_signing" {
  name = "lux/apple-signing"
}

resource "aws_iam_role" "release_signing" {
  name = "universal-tiling-window-manager-release-signing"
  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect    = "Allow"
      Principal = { Federated = data.aws_iam_openid_connect_provider.github.arn }
      Action    = "sts:AssumeRoleWithWebIdentity"
      Condition = {
        StringEquals = {
          "token.actions.githubusercontent.com:aud" = "sts.amazonaws.com"
          # Main only, never repo:…:*: the updater key signs what installed
          # copies install, so no branch or pull request run may read it.
          "token.actions.githubusercontent.com:sub" = "repo:johncarmack1984/universal-tiling-window-manager:ref:refs/heads/main"
        }
      }
    }]
  })
}

resource "aws_iam_role_policy" "read_signing_material" {
  name = "read-signing-material"
  role = aws_iam_role.release_signing.id
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect = "Allow"
      Action = "secretsmanager:GetSecretValue"
      Resource = [
        data.aws_secretsmanager_secret.updater_signing_key.arn,
        data.aws_secretsmanager_secret.apple_signing.arn,
      ]
    }]
  })
}

output "release_signing_role_arn" {
  description = "The role release.yml assumes (role-to-assume)."
  value       = aws_iam_role.release_signing.arn
}
