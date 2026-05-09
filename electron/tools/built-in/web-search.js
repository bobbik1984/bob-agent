module.exports = {
  name: 'web_search',
  description: 'Searches the web using Tavily API to get up-to-date information.',
  parameters: {
    type: 'object',
    properties: {
      query: {
        type: 'string',
        description: 'The query to search for.'
      },
      max_results: {
        type: 'number',
        description: 'The maximum number of results to return. Defaults to 5.'
      }
    },
    required: ['query']
  },
  async execute({ query, max_results = 5 }) {
    try {
      const apiKey = process.env.TAVILY_API_KEY;
      if (!apiKey) {
        return 'Error: TAVILY_API_KEY environment variable is not set.';
      }

      const response = await fetch('https://api.tavily.com/search', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json'
        },
        body: JSON.stringify({
          api_key: apiKey,
          query: query,
          max_results: max_results,
          search_depth: 'basic'
        })
      });

      if (!response.ok) {
        return `Error: API request failed with status ${response.status}`;
      }

      const data = await response.json();
      return JSON.stringify(data, null, 2);
    } catch (err) {
      return `Error: ${err.message}`;
    }
  }
};
