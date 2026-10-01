window.__bobBootDiag?.('entry-import-begin');

import('./main.js')
  .then(() => window.__bobBootDiag?.('entry-import-complete'))
  .catch(() => {
    window.__bobBootDiag?.('entry-import-failed');
  });
