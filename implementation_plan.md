# Enhance Native Google Calendar Integration

To provide a seamless, packaged-app experience for Google Calendar without requiring terminal commands, we will fully activate Bob's built-in native Google Calendar connector and deprecate the third-party MCP workaround.

## Proposed Changes

### 1. Backend: src-tauri/src/google_calendar.rs
- Modify start_google_oauth to read the client_id from stored credentials (or fallback to empty) instead of the hardcoded BOB_GOOGLE_CLIENT_ID. If no client_id is found, return an error requesting configuration.
- Modify exchange_code_for_token to read the client_id and client_secret from stored credentials.

### 2. Frontend: src/views/settings/SettingsConnections.vue
- Add a configuration form for google (similar to the existing lark form) allowing the user to input their Client ID and Client Secret.
- Add a "Save" button that calls connectorSaveCredentials('google', { client_id, client_secret }).
- Remove the ddGoogleCalendarMcp (MCP Quick Add) button to avoid confusing users with two Google Calendar options.
- Ensure the "Connect" button triggers the native connectOAuth('google') flow.

## User Review Required
> [!IMPORTANT]
> The built-in Google Calendar connection will require you to provide your own Google Cloud Console OAuth Client ID and Secret (Desktop App type). Are you okay with configuring this in the Settings UI?

