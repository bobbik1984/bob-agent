const { BaseTool } = require('../base');

class UpdateEventTool extends BaseTool {
  constructor() {
    super();
    this.name = 'update_event';
    this.description = 'Update the status, title, or time of an existing calendar event or to-do item in the local database.';
    this.input_schema = {
      type: 'object',
      properties: {
        id: {
          type: 'number',
          description: 'The unique ID of the event to update.'
        },
        status: {
          type: 'string',
          enum: ['pending', 'done', 'cancelled'],
          description: 'The new status of the event.'
        },
        start_time: {
          type: 'string',
          description: 'The new start time in YYYY-MM-DD HH:mm:ss format.'
        },
        end_time: {
          type: 'string',
          description: 'The new end time in YYYY-MM-DD HH:mm:ss format.'
        }
      },
      required: ['id']
    };
  }

  async execute(params) {
    try {
      if (!global.db) throw new Error("Database instance not available");
      
      const { id, status, start_time, end_time } = params;
      
      let updated = false;
      if (status) {
        global.db.updateEventStatus(id, status);
        updated = true;
      }
      
      if (start_time || end_time) {
        // We assume global.db has updateEventTime method.
        // If not, we might need to add it to db.js. But standard update is status.
        if (typeof global.db.updateEventTime === 'function') {
          global.db.updateEventTime(id, start_time, end_time);
          updated = true;
        } else {
           return `Warning: Status was updated, but time update is not fully supported by the current DB schema.`;
        }
      }
      
      if (!updated) {
        return "No changes were specified to update.";
      }
      
      return `Successfully updated event ID ${id}.`;
    } catch (err) {
      return `Failed to update event: ${err.message}`;
    }
  }
}

module.exports = new UpdateEventTool();
