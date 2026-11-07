// Negative receipts require the same request and session, never absence in history.
export function isNotAcceptedReceipt(receipt, session, request) {
 return receipt?.session_id === session && receipt?.request_id === request
  && (receipt.status === 'not_accepted' || (receipt.status === 'accepted' && receipt.execution_status === 'not_accepted'));
}
