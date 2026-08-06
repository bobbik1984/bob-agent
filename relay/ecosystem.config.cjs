module.exports = {
  apps: [{
    name: 'bob-relay',
    script: 'src/server.js',
    instances: 1,
    autorestart: true,
    watch: false,
    max_memory_restart: '200M',
    env: { NODE_ENV: 'development', PORT: 3090 },
    env_production: { NODE_ENV: 'production', PORT: 3090 },
    log_date_format: 'YYYY-MM-DD HH:mm:ss Z',
    error_file: 'logs/error.log',
    out_file: 'logs/out.log',
    merge_logs: true,
  }],
};
