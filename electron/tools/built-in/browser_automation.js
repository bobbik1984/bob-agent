const obscuraDriver = require('../../services/obscura-driver.js');
const { BaseTool } = require('../base');

class BrowserAutomationTool extends BaseTool {
  constructor() {
    super();
    this.name = 'browser_automation';
    this.description = 'Automate web browsing using the stealth Obscura browser. Use this tool to navigate, extract page contents, click elements, or type into forms.';
    this.input_schema = {
      type: 'object',
      properties: {
        action: {
          type: 'string',
          enum: ['navigate', 'get_html', 'click', 'type', 'evaluate_js'],
          description: 'The browser action to perform. ' +
                       'navigate: go to a URL. ' +
                       'get_html: extract page text and actionable elements. ' +
                       'click: click an element by CSS selector. ' +
                       'type: type text into an input field by CSS selector. ' +
                       'evaluate_js: run JavaScript.'
        },
        url: {
          type: 'string',
          description: 'The URL to navigate to. Required if action is navigate.'
        },
        selector: {
          type: 'string',
          description: 'The CSS selector of the element. Required if action is click or type. Example: "#submit-btn" or "input[name=\'q\']"'
        },
        text: {
          type: 'string',
          description: 'The text to type. Required if action is type.'
        },
        expression: {
          type: 'string',
          description: 'The JavaScript expression to evaluate. Required if action is evaluate_js.'
        }
      },
      required: ['action']
    };
  }

  async execute(params) {
    const { action, url, selector, text, expression } = params;

    try {
      let result = '';
      switch (action) {
        case 'navigate':
          if (!url) return JSON.stringify({ error: 'Missing "url" for navigate action' });
          const navResult = await obscuraDriver.navigate(url);
          const domAfterNav = await obscuraDriver.getHtml();
          result = JSON.stringify({
            status: navResult,
            page_content: domAfterNav
          });
          break;
          
        case 'get_html':
          const dom = await obscuraDriver.getHtml();
          result = JSON.stringify(dom);
          break;
          
        case 'click':
          if (!selector) return JSON.stringify({ error: 'Missing "selector" for click action' });
          const clickResult = await obscuraDriver.click(selector);
          await new Promise(r => setTimeout(r, 1500));
          const domAfterClick = await obscuraDriver.getHtml();
          result = JSON.stringify({
            status: clickResult,
            page_content: domAfterClick
          });
          break;
          
        case 'type':
          if (!selector) return JSON.stringify({ error: 'Missing "selector" for type action' });
          if (text === undefined) return JSON.stringify({ error: 'Missing "text" for type action' });
          const typeResult = await obscuraDriver.type(selector, text);
          result = JSON.stringify(typeResult);
          break;
          
        case 'evaluate_js':
          if (!expression) return JSON.stringify({ error: 'Missing "expression" for evaluate_js action' });
          const evalResult = await obscuraDriver.evaluateJs(expression);
          result = JSON.stringify(evalResult);
          break;
          
        default:
          result = JSON.stringify({ error: `Unknown action: ${action}` });
      }

      return `<untrusted_web_content source="browser_automation">\n${result}\n</untrusted_web_content>`;
    } catch (err) {
      console.error('[BrowserAutomationTool] Error:', err);
      return JSON.stringify({ error: err.message });
    }
  }
}

module.exports = new BrowserAutomationTool();
