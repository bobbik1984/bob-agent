/**
 * 诊断脚本：模拟 useChat.js 的渲染管线（不依赖 jsdom）
 * 逐步追踪 Markdown → HTML → regex replace 的每一步输出
 */
const { marked } = require('marked');

// 模拟 Bob 的几种典型图片输出
const testCases = [
  '![bob-agent 图标](file:///D:\\OneDrive\\Learning\\Code\\Gemini\\bob-agent\\src-tauri\\icons\\icon.png)',
  '![bob-agent 图标](file:///D:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/icons/icon.png)',
  '![icon](D:\\OneDrive\\Learning\\Code\\Gemini\\bob-agent\\src-tauri\\icons\\icon.png)',
  '好的！\n\n![bob-agent 图标](file:///D:/OneDrive/Learning/Code/Gemini/bob-agent/src-tauri/icons/icon.png)\n\n现在能看到吗？',
];

for (let i = 0; i < testCases.length; i++) {
  const md = testCases[i];
  console.log(`\n${'='.repeat(70)}`);
  console.log(`TEST CASE ${i + 1}:`);
  console.log(`  Input: ${md.substring(0, 100)}`);
  console.log(`${'='.repeat(70)}`);

  // Step 1: marked.parse()
  const step1 = marked.parse(md);
  console.log(`\n  Step 1 (marked.parse):`);
  console.log(`  ${step1.trim()}`);

  // Check if <img> tag exists
  const imgMatch = step1.match(/<img[^>]*>/);
  console.log(`  → Contains <img>: ${imgMatch ? '✅ YES' : '❌ NO'}`);
  if (imgMatch) {
    console.log(`  → img tag: ${imgMatch[0]}`);
  }

  // Step 2: regex replacement (from useChat.js line 421)
  let step2 = step1;
  step2 = step2.replace(
    /(<img\s+[^>]*src=")(?:file:\/\/\/)?([A-Za-z]:(?:[\\\/]|%5[Cc]|%2[Ff])[^"]+)(")/gi,
    (_, pre, path, post) => {
      console.log(`\n  ✅ REGEX MATCHED!`);
      console.log(`    pre:  ${pre}`);
      console.log(`    path: ${path}`);
      console.log(`    post: ${post}`);
      return pre + 'http://bob.localhost/' + path.replace(/\\/g, '/') + post;
    }
  );
  console.log(`\n  Step 2 (regex replace result):`);
  console.log(`  ${step2.trim()}`);
  
  const changed = step1 !== step2;
  console.log(`  → Changed by regex: ${changed ? '✅ YES' : '❌ NO - REGEX DID NOT MATCH!'}`);
  
  if (!changed && imgMatch) {
    // Debug: extract src from img tag
    const srcMatch = imgMatch[0].match(/src="([^"]+)"/);
    if (srcMatch) {
      console.log(`  → Actual src value: ${srcMatch[1]}`);
      console.log(`  → Char codes of first 10 chars after drive letter:`);
      const afterDrive = srcMatch[1].replace(/^.*?[A-Za-z]:/, '');
      for (let j = 0; j < Math.min(10, afterDrive.length); j++) {
        console.log(`    [${j}] '${afterDrive[j]}' = U+${afterDrive.charCodeAt(j).toString(16).padStart(4, '0')}`);
      }
    }
  }
}
