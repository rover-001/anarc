const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

// Load indexer script
const indexerJs = fs.readFileSync(path.join(__dirname, '../../src/agent/indexer.js'), 'utf-8');
const fixturePath = path.join(__dirname, 'fixtures/email_inbox.html');

console.log('Testing Anarc Agent Control on Email Inbox fixture...');
console.log('Fixture:', fixturePath);

// Execute with chromium headless using DevTools protocol
const script = `
const puppeteer = require('puppeteer');
(async () => {
  const browser = await puppeteer.launch({
    executablePath: '/usr/bin/chromium',
    headless: 'new',
    args: ['--no-sandbox', '--disable-setuid-sandbox']
  });
  const page = await browser.newPage();
  await page.setViewport({ width: 1280, height: 800 });
  await page.goto('file://${fixturePath}');

  // Inject Anarc Indexer
  await page.evaluate(${JSON.stringify(indexerJs)});

  // 1. Tool: see (inspect inbox)
  const seeJson = await page.evaluate(() => window.__anarc_index('view'));
  const parsedSee = JSON.parse(seeJson);

  console.log('=== [1] ANARC SEE OUTPUT (Inbox View) ===');
  console.log('Header:', parsedSee.title, '| URL:', parsedSee.url);
  console.log('Interactive Elements Found:', parsedSee.elements.length);

  parsedSee.elements.slice(0, 15).forEach((el, idx) => {
    const flags = el.flags.length > 0 ? ' [' + el.flags.join(', ') + ']' : '';
    console.log(\`  \${el.kind}\${idx + 1} "\${el.name}"\${flags}\`);
  });

  // 2. Tool: read (extract email summary table)
  const readText = await page.evaluate(() => window.__anarc_read('text', 2000));
  console.log('\\n=== [2] ANARC READ OUTPUT (Readable Content) ===');
  console.log(readText.split('\\n').slice(0, 12).join('\\n'));

  // 3. Tool: do (open email 1)
  console.log('\\n=== [3] ANARC DO (Click First Email) ===');
  await page.evaluate(() => openEmail(1));

  // Inspect detail view
  const emailBody = await page.evaluate(() => window.__anarc_read('text', 1000));
  console.log('Email 1 Content:');
  console.log(emailBody);

  await browser.close();
})();
`;

fs.writeFileSync('/tmp/anarc_email_test.js', script);
try {
  const output = execSync('npx -y puppeteer-core /tmp/anarc_email_test.js', { encoding: 'utf-8' });
  console.log(output);
} catch (e) {
  console.error('Puppeteer run error:', e.message);
}
