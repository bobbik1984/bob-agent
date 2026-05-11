const { BaseTool } = require('../base');

class SearchEventsTool extends BaseTool {
  constructor() {
    super();
    this.name = 'search_events';
    this.description = 'Search the local calendar database for events within a specific time range. Use this to check the user\'s schedule or to-do list.';
    this.input_schema = {
      type: 'object',
      properties: {
        start_date: {
          type: 'string',
          description: 'The start date for the search range in YYYY-MM-DD format (e.g. 2026-05-11).'
        },
        end_date: {
          type: 'string',
          description: 'The end date for the search range in YYYY-MM-DD format.'
        }
      },
      required: ['start_date', 'end_date']
    };
  }

  async execute(params) {
    try {
      if (!global.db) throw new Error("Database instance not available");
      
      const events = global.db.getEvents(params.start_date, params.end_date);
      if (events.length === 0) {
        return `No events found between ${params.start_date} and ${params.end_date}.`;
      }
      return JSON.stringify(events, null, 2);
    } catch (err) {
      return `Failed to search events: ${err.message}`;
    }
  }
}

module.exports = new SearchEventsTool();
