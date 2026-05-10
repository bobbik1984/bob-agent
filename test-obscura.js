const driver = require('./electron/services/obscura-driver.js');

async function test() {
  console.log('Testing ObscuraDriver...');
  
  try {
    const navResult = await driver.navigate('https://www.wikipedia.org');
    console.log('Navigate result:', navResult);
    
    const dom = await driver.getHtml(500);
    console.log('DOM extraction result:', dom);
    if (!dom || dom.error) {
       console.error('Failed to get HTML:', dom);
    } else {
       console.log('DOM extracted, title:', dom.title);
       console.log('DOM actionable elements count:', dom.actionable ? dom.actionable.length : 0);
    }
    
    await driver.close();
    console.log('Test complete!');
    process.exit(0);
  } catch (err) {
    console.error('Test failed:', err);
    await driver.close();
    process.exit(1);
  }
}

test();
