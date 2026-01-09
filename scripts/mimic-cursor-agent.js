#!/usr/bin/env node

/**
 * Mimic Cursor Agent Authentication
 * 
 * Simply copies and uses the real Cursor agent authentication
 * No spoofing - just direct mimicry of agent's auth
 */

const fs = require('fs');
const path = require('path');
const os = require('os');

function mimicCursorAgent() {
  console.log('🎭 Mimicking Cursor Agent Authentication...');
  
  const homeDir = os.homedir();
  const cursorDir = path.join(homeDir, '.cursor');
  const opDbusDir = '/etc/op-dbus';
  
  // Ensure op-dbus dir exists
  if (!fs.existsSync(opDbusDir)) {
    fs.mkdirSync(opDbusDir, { recursive: true });
  }
  
  // Copy Cursor auth files directly
  const cursorFiles = [
    'auth.json',
    'config.json', 
    'enterprise.json',
    'tokens.json',
    'credentials.json'
  ];
  
  let copiedFiles = 0;
  
  cursorFiles.forEach(file => {
    const sourcePath = path.join(cursorDir, file);
    const destPath = path.join(opDbusDir, `cursor-${file}`);
    
    if (fs.existsSync(sourcePath)) {
      try {
        // Read and parse to ensure it's valid JSON
        const content = fs.readFileSync(sourcePath, 'utf8');
        JSON.parse(content); // Validate JSON
        
        // Copy file
        fs.copyFileSync(sourcePath, destPath);
        console.log(`✅ Copied: ${file} → cursor-${file}`);
        copiedFiles++;
      } catch (e) {
        console.log(`⚠️  Skipped ${file}: ${e.message}`);
      }
    }
  });
  
  if (copiedFiles === 0) {
    console.log('❌ No Cursor auth files found to copy');
    console.log('Make sure Cursor is installed and you have enterprise auth set up');
    return;
  }
  
  // Create symlink to primary auth file
  const primaryAuth = path.join(opDbusDir, 'cursor-auth.json');
  if (fs.existsSync(primaryAuth)) {
    const symlinkPath = path.join(opDbusDir, 'mimicked-cursor-auth.json');
    try {
      if (fs.existsSync(symlinkPath)) fs.unlinkSync(symlinkPath);
      fs.symlinkSync(primaryAuth, symlinkPath);
      console.log('🔗 Created symlink for mimicked auth');
    } catch (e) {
      console.log('⚠️  Could not create symlink');
    }
  }
  
  // Extract key authentication data
  let authData = {};
  try {
    const authFile = path.join(opDbusDir, 'cursor-auth.json');
    if (fs.existsSync(authFile)) {
      authData = JSON.parse(fs.readFileSync(authFile, 'utf8'));
    }
  } catch (e) {
    console.log('⚠️  Could not read auth data');
  }
  
  // Create environment script that mimics Cursor agent
  const envScript = path.join(opDbusDir, 'mimic-cursor-env.sh');
  const envContent = `
# Mimic Cursor Agent Environment
export MIMICKING_CURSOR_AGENT=true
export CURSOR_AUTH_CONFIG="${path.join(opDbusDir, 'cursor-auth.json')}"
export ENTERPRISE_TOKEN="${authData?.enterprise?.token || ''}"
export GCP_PROJECT="${authData?.enterprise?.project || authData?.gcp?.project_id || ''}"
export VERTEX_AUTH_TOKEN="${authData?.vertex?.token || ''}"
export AGENT_AUTH_MODE=cursor
export ENTERPRISE_BILLING=true

# Copy Cursor's environment variables
${Object.entries(authData?.env || {}).map(([k, v]) => `export ${k}="${v}"`).join('\n')}

echo "🎭 CLI now mimicking Cursor Agent"
echo "Project: $GCP_PROJECT"
echo "Enterprise Token: ${ENTERPRISE_TOKEN.substring(0, 20)}..."
echo "Billing: Enterprise"
`;
  
  fs.writeFileSync(envScript, envContent);
  console.log(`🔧 Created mimic environment: ${envScript}`);
  
  // Create systemd drop-in to load this on startup
  const systemdDropIn = '/etc/systemd/system/op-web.service.d/mimic-cursor.conf';
  const systemdDir = path.dirname(systemdDropIn);
  
  if (!fs.existsSync(systemdDir)) {
    fs.mkdirSync(systemdDir, { recursive: true });
  }
  
  const dropInContent = `
[Service]
EnvironmentFile=${envScript}
`;
  
  fs.writeFileSync(systemdDropIn, dropInContent);
  console.log(`⚙️  Created systemd drop-in: ${systemdDropIn}`);
  
  console.log('\n🎯 Mimicry Complete!');
  console.log('📋 To activate:');
  console.log(`   source ${envScript}`);
  console.log('   sudo systemctl daemon-reload');
  console.log('   sudo systemctl restart op-web');
  
  console.log('\n🔍 The CLI will now use the EXACT SAME authentication as your Cursor agent!');
  console.log('💰 This gives you enterprise billing instead of API charges!');
}

try {
  mimicCursorAgent();
} catch (error) {
  console.error('❌ Mimicry failed:', error.message);
  process.exit(1);
}
