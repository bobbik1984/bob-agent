window.__bobBootDiag?.('entry-import-begin');

import('./main.js')
  .then(() => window.__bobBootDiag?.('entry-import-complete'))
  .catch((error) => {
    window.__bobBootDiag?.(`entry-import-failed:${error?.name || 'unknown'}`);
  });
