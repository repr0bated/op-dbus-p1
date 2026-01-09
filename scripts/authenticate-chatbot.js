#!/usr/bin/env node

/**
 * Puppeteer Authentication Script for Chatbot
 *
 * This script automates the login process for Gemini Code Assist
 * and extracts authentication tokens for the chatbot to use.
 */

const puppeteer = require('puppeteer');

async function authenticateChatbot() {
  console.log('🚀 Starting chatbot authentication with Puppeteer...');

  const browser = await puppeteer.launch({
    headless: true,
    args: ['--no-sandbox', '--disable-setuid-sandbox']
  });

  try {
    const page = await browser.newPage();

    // Set up console logging
    page.on('console', msg => console.log('PAGE LOG:', msg.text()));

    console.log('📱 Navigating to Code Assist...');
    await page.goto('https://codeassist.google.com', {
      waitUntil: 'networkidle2',
      timeout: 30000
    });

    // Wait for and handle enterprise login
    console.log('🔐 Looking for enterprise login option...');

    // Try different selectors for enterprise login
    const enterpriseSelectors = [
      '[data-testid="enterprise-login"]',
      '#enterprise-login',
      'button:has-text("Enterprise")',
      'a:has-text("Enterprise")',
      '[href*="enterprise"]',
      '.enterprise-login'
    ];

    let enterpriseFound = false;
    for (const selector of enterpriseSelectors) {
      try {
        await page.waitForSelector(selector, { timeout: 5000 });
        await page.click(selector);
        console.log(`✅ Found and clicked enterprise login: ${selector}`);
        enterpriseFound = true;
        break;
      } catch (e) {
        // Selector not found, continue
      }
    }

    if (!enterpriseFound) {
      console.log('⚠️  Enterprise login not found, trying direct GCP project setup...');

      // Look for project input or settings
      const projectSelectors = [
        '[data-testid="project-input"]',
        '#project-input',
        'input[placeholder*="project"]',
        'input[name*="project"]',
        '.project-input'
      ];

      for (const selector of projectSelectors) {
        try {
          await page.waitForSelector(selector, { timeout: 3000 });
          await page.type(selector, '420281222188');
          console.log(`✅ Entered GCP project: 420281222188 in ${selector}`);
          break;
        } catch (e) {
          // Continue trying selectors
        }
      }
    }

    // Wait for authentication to complete
    console.log('⏳ Waiting for authentication...');
    await page.waitForTimeout(10000);

    // Try to extract auth tokens
    console.log('🔑 Extracting authentication tokens...');

    const authData = await page.evaluate(() => {
      const tokens = {};

      // Check localStorage for auth tokens
      for (let i = 0; i < localStorage.length; i++) {
        const key = localStorage.key(i);
        if (key && (key.includes('token') || key.includes('auth') || key.includes('access'))) {
          tokens[key] = localStorage.getItem(key);
        }
      }

      // Check sessionStorage
      for (let i = 0; i < sessionStorage.length; i++) {
        const key = sessionStorage.key(i);
        if (key && (key.includes('token') || key.includes('auth') || key.includes('access'))) {
          tokens[key] = sessionStorage.getItem(key);
        }
      }

      // Look for tokens in cookies
      const cookies = document.cookie.split(';').reduce((acc, cookie) => {
        const [key, value] = cookie.trim().split('=');
        if (key && (key.includes('token') || key.includes('auth'))) {
          acc[key] = value;
        }
        return acc;
      }, {});

      return { localStorage: tokens, sessionStorage: {}, cookies };
    });

    console.log('📋 Found authentication data:');
    console.log(JSON.stringify(authData, null, 2));

    // Save auth data for chatbot
    const fs = require('fs');
    const authFile = '/etc/op-dbus/chatbot-auth.json';

    fs.writeFileSync(authFile, JSON.stringify({
      timestamp: new Date().toISOString(),
      gcp_project: '420281222188',
      auth_data: authData
    }, null, 2));

    console.log(`💾 Saved authentication data to: ${authFile}`);

    // Test the authentication by making a sample request
    console.log('🧪 Testing authentication with a sample API call...');

    // Extract access token if available
    let accessToken = null;
    Object.values(authData.localStorage).forEach(token => {
      if (token && token.length > 20 && token.includes('.')) {
        accessToken = token; // Likely a JWT
      }
    });

    if (accessToken) {
      console.log('🔐 Found potential access token, testing API call...');

      // Test Vertex AI access (this should work with enterprise auth)
      const testResponse = await fetch('https://us-central1-aiplatform.googleapis.com/v1/projects/420281222188/locations/us-central1/publishers/google/models', {
        headers: {
          'Authorization': `Bearer ${accessToken}`,
          'Content-Type': 'application/json'
        }
      });

      if (testResponse.ok) {
        console.log('✅ Authentication successful! Enterprise API access confirmed.');
      } else {
        console.log(`⚠️  API test failed: ${testResponse.status} ${testResponse.statusText}`);
      }
    }

    console.log('🎉 Authentication process complete!');

  } catch (error) {
    console.error('❌ Authentication failed:', error.message);
    process.exit(1);
  } finally {
    await browser.close();
  }
}

// Run the authentication
authenticateChatbot().catch(console.error);