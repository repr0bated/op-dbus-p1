#!/bin/bash

# Google Cloud Application Default Credentials (ADC) Setup
# This file configures ADC and API keys for the op-dbus-v2 project

echo "Setting up Google Cloud ADC and API keys..."

# Google Cloud Project Configuration
export GOOGLE_CLOUD_PROJECT="geminidev-479406"
export GOOGLE_CLOUD_LOCATION="global"
export GOOGLE_GENAI_USE_VERTEXAI=true
export GOOGLE_GENAI_MODEL="gemini-3-pro-preview"

# Source API key from ~/.bashrc if it exists
if [ -f ~/.bashrc ]; then
    # Extract GEMINI_API_KEY from ~/.bashrc (note: there's a typo as GOPOGLE_API_KEY)
    GEMINI_API_KEY_FROM_BASHRC=$(grep -oP "GOPOGLE_API_KEY='\K[^']*" ~/.bashrc 2>/dev/null || echo "")
    if [ -n "$GEMINI_API_KEY_FROM_BASHRC" ]; then
        export GEMINI_API_KEY="$GEMINI_API_KEY_FROM_BASHRC"
    fi
fi

# Application Default Credentials
# Option 1: Service Account Key File (uncomment and set path)
# export GOOGLE_APPLICATION_CREDENTIALS="/path/to/your/service-account-key.json"

# Option 2: Use gcloud ADC (run: gcloud auth application-default login)
# This will use credentials from gcloud auth application-default login

# Google Cloud SDK Configuration
export CLOUDSDK_CORE_PROJECT="$GOOGLE_CLOUD_PROJECT"
export CLOUDSDK_COMPUTE_REGION="$GOOGLE_CLOUD_LOCATION"

# API Keys for LLM Providers
# Set these to your actual API keys
export GEMINI_API_KEY="${GEMINI_API_KEY:-}"
export ANTHROPIC_API_KEY="${ANTHROPIC_API_KEY:-}"
export PERPLEXITY_API_KEY="${PERPLEXITY_API_KEY:-}"
export HF_TOKEN="${HF_TOKEN:-}"

# Additional Google Cloud Environment Variables
export GOOGLE_CLOUD_STORAGE_BUCKET="${GOOGLE_CLOUD_STORAGE_BUCKET:-}"
export GOOGLE_CLOUD_PUBSUB_TOPIC="${GOOGLE_CLOUD_PUBSUB_TOPIC:-}"

# Verify ADC setup
echo "------------------------------------------------"
echo "Google Cloud ADC Configuration:"
echo "Project: $GOOGLE_CLOUD_PROJECT"
echo "Location: $GOOGLE_CLOUD_LOCATION"
echo "Vertex AI Enabled: $GOOGLE_GENAI_USE_VERTEXAI"
echo "Service Account Key: ${GOOGLE_APPLICATION_CREDENTIALS:-Not set (using gcloud ADC)}"
echo "------------------------------------------------"

# Function to verify gcloud authentication
verify_gcloud_auth() {
    if command -v gcloud &> /dev/null; then
        echo "Checking gcloud authentication..."
        if gcloud auth list --filter=status:ACTIVE --format="value(account)" | head -n 1 > /dev/null; then
            echo "✓ gcloud authentication verified"
        else
            echo "⚠ No active gcloud authentication found"
            echo "Run: gcloud auth application-default login"
        fi
    else
        echo "⚠ gcloud CLI not found"
    fi
}

# Function to verify ADC
verify_adc() {
    if [ -n "$GOOGLE_APPLICATION_CREDENTIALS" ]; then
        if [ -f "$GOOGLE_APPLICATION_CREDENTIALS" ]; then
            echo "✓ Service account key file found"
        else
            echo "✗ Service account key file not found: $GOOGLE_APPLICATION_CREDENTIALS"
        fi
    else
        echo "Using gcloud Application Default Credentials"
        verify_gcloud_auth
    fi
}

# Run verification if not in a non-interactive shell
if [ -t 0 ]; then
    verify_adc
fi

echo "ADC setup complete. Ready to use Google Cloud services."
echo "------------------------------------------------"