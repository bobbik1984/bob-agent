import test from 'node:test';
import assert from 'node:assert/strict';
import {previewErrorKey} from '../src/preview-error.mjs';
test('missing file and ambiguous context are not reported as node offline',()=>{
 assert.equal(previewErrorKey('DSH_FILE_NOT_FOUND'),'filePreviewNotFound');
 assert.equal(previewErrorKey('DSH_FILE_CONTEXT_AMBIGUOUS'),'filePreviewContextAmbiguous');
 assert.equal(previewErrorKey('DSH_FILE_TIMEOUT'),'filePreviewTimeout');
 assert.equal(previewErrorKey('DSH_UNAVAILABLE'),'filePreviewFailed');
});
