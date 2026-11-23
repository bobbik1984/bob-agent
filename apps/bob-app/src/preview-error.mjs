export function previewErrorKey(code) {
  return ({DSH_FILE_NOT_FOUND:'filePreviewNotFound', DSH_FILE_CONTEXT_AMBIGUOUS:'filePreviewContextAmbiguous', DSH_FILE_TIMEOUT:'filePreviewTimeout', DSH_FILE_UNAVAILABLE:'filePreviewReadFailed'})[code] || 'filePreviewFailed';
}
