module.exports = {
  name: 'weather',
  description: '查询指定城市的实时天气和未来几天预报。当用户询问天气信息时使用。',
  parameters: {
    type: 'object',
    properties: {
      city: {
        type: 'string',
        description: '城市名称（可以使用拼音或英文，如 "Beijing", "Shanghai", "London"）'
      }
    },
    required: ['city']
  },
  execute: async (args) => {
    const city = encodeURIComponent(args.city);
    const url = `https://wttr.in/${city}?format=j1`;
    
    try {
      const response = await fetch(url);
      if (!response.ok) {
        return `查询天气失败: HTTP ${response.status} ${response.statusText}`;
      }
      const data = await response.json();
      
      const current = data.current_condition[0];
      const weatherDesc = current.lang_zh ? current.lang_zh[0].value : current.weatherDesc[0].value;
      
      const result = {
        city: data.nearest_area[0].areaName[0].value,
        country: data.nearest_area[0].country[0].value,
        current: {
          temp_c: current.temp_C,
          feels_like_c: current.FeelsLikeC,
          humidity: current.humidity + '%',
          description: weatherDesc,
          wind: `${current.winddir16Point} ${current.windspeedKmph} km/h`
        },
        forecast: data.weather.slice(0, 3).map(day => ({
          date: day.date,
          max_temp: day.maxtempC,
          min_temp: day.mintempC,
          sunrise: day.astronomy[0].sunrise,
          sunset: day.astronomy[0].sunset
        }))
      };
      
      return result;
    } catch (err) {
      return `查询天气请求发生错误: ${err.message}`;
    }
  }
};
