terraform {
  required_version = ">= 1.10"

  backend "s3" {
    bucket       = "john-carmack-terraform-state"
    key          = "universal-tiling-window-manager/terraform.tfstate"
    region       = "us-west-2"
    use_lockfile = true
  }

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 6.0"
    }
  }
}

# The secrets live in us-west-1; IAM is global.
provider "aws" {
  region = "us-west-1"
}
