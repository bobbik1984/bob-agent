# ECharts MBB+ Brand Themes

> 仅当 Storyboard slide 包含 `chart` 字段时加载此文件。

---

## 使用方式

在 HTML `<head>` 中引入 ECharts CDN 后，注册品牌主题，然后用 `echarts.init(dom, 'mbb-{brand}')` 初始化。

```html
<script src="https://cdn.jsdelivr.net/npm/echarts@5/dist/echarts.min.js"></script>
<script>
// 在 Reveal.js ready 回调中注册主题并初始化图表
</script>
```

---

## 1. McKinsey Theme

```javascript
echarts.registerTheme('mbb-mckinsey', {
  color: ['#005EB8', '#052243', '#FFDB00', '#7CB5D2', '#333333', '#888888', '#D9534F'],
  backgroundColor: 'transparent',
  textStyle: { fontFamily: 'Inter, Microsoft YaHei, sans-serif' },
  title: { textStyle: { color: '#052243', fontWeight: 700, fontSize: 16 } },
  legend: { textStyle: { color: '#666666' } },
  categoryAxis: {
    axisLine: { lineStyle: { color: '#E5E7EB' } },
    axisLabel: { color: '#666666' },
    splitLine: { show: false }
  },
  valueAxis: {
    axisLine: { show: false },
    axisLabel: { color: '#666666' },
    splitLine: { lineStyle: { color: '#F0F0F0' } }
  }
});
```

---

## 2. BCG Theme

```javascript
echarts.registerTheme('mbb-bcg', {
  color: ['#00A86B', '#006B3F', '#FFDB00', '#7CCEA8', '#4A4A4A', '#888888', '#D9534F'],
  backgroundColor: 'transparent',
  textStyle: { fontFamily: 'Inter, Microsoft YaHei, sans-serif' },
  title: { textStyle: { color: '#006B3F', fontWeight: 700, fontSize: 16 } },
  legend: { textStyle: { color: '#4A4A4A' } },
  categoryAxis: {
    axisLine: { lineStyle: { color: '#E5E7EB' } },
    axisLabel: { color: '#4A4A4A' },
    splitLine: { show: false }
  },
  valueAxis: {
    axisLine: { show: false },
    axisLabel: { color: '#4A4A4A' },
    splitLine: { lineStyle: { color: '#F0F0F0' } }
  }
});
```

---

## 3. Bain Theme

```javascript
echarts.registerTheme('mbb-bain', {
  color: ['#CB2026', '#1A1A1A', '#E8A0A0', '#6B6B6B', '#333333', '#888888', '#2E7D32'],
  backgroundColor: 'transparent',
  textStyle: { fontFamily: 'Inter, Microsoft YaHei, sans-serif' },
  title: { textStyle: { color: '#1A1A1A', fontWeight: 700, fontSize: 16 } },
  legend: { textStyle: { color: '#6B6B6B' } },
  categoryAxis: {
    axisLine: { lineStyle: { color: '#E5E7EB' } },
    axisLabel: { color: '#6B6B6B' },
    splitLine: { show: false }
  },
  valueAxis: {
    axisLine: { show: false },
    axisLabel: { color: '#6B6B6B' },
    splitLine: { lineStyle: { color: '#F0F0F0' } }
  }
});
```

---

## 4. Roland Berger Theme

```javascript
echarts.registerTheme('mbb-roland-berger', {
  color: ['#1E5FA6', '#FFB800', '#7BA3CC', '#555555', '#333333', '#888888', '#D9534F'],
  backgroundColor: 'transparent',
  textStyle: { fontFamily: 'Inter, Microsoft YaHei, sans-serif' },
  title: { textStyle: { color: '#1E5FA6', fontWeight: 700, fontSize: 16 } },
  legend: { textStyle: { color: '#555555' } },
  categoryAxis: {
    axisLine: { lineStyle: { color: '#E5E7EB' } },
    axisLabel: { color: '#555555' },
    splitLine: { show: false }
  },
  valueAxis: {
    axisLine: { show: false },
    axisLabel: { color: '#555555' },
    splitLine: { lineStyle: { color: '#F0F0F0' } }
  }
});
```

---

## 5. 图表渲染模板

### 柱状图 (bar)

```javascript
const option = {
  title: { text: chart.title },
  tooltip: { trigger: 'axis' },
  xAxis: { type: 'category', data: chart.data.map(d => d.label) },
  yAxis: { type: 'value' },
  series: [{
    type: 'bar',
    data: chart.data.map(d => ({
      value: d.value,
      itemStyle: d.highlight ? { color: 'var(--brand-accent)' } : {}
    })),
    barWidth: '50%'
  }]
};
```

### 瀑布图 (waterfall)

```javascript
// 瀑布图通过堆叠柱实现：透明底座 + 实色柱体
const labels = chart.data.map(d => d.label);
const values = chart.data.map(d => d.value);

let cumulative = 0;
const base = [], positive = [], negative = [];

values.forEach((v, i) => {
  if (chart.data[i].isTotal) {
    base.push(0);
    positive.push(v > 0 ? v : 0);
    negative.push(v < 0 ? Math.abs(v) : 0);
  } else {
    if (v >= 0) {
      base.push(cumulative);
      positive.push(v);
      negative.push(0);
    } else {
      base.push(cumulative + v);
      positive.push(0);
      negative.push(Math.abs(v));
    }
    cumulative += v;
  }
});

const option = {
  title: { text: chart.title },
  xAxis: { type: 'category', data: labels },
  yAxis: { type: 'value' },
  series: [
    { type: 'bar', stack: 'w', data: base, itemStyle: { color: 'transparent' }, emphasis: { itemStyle: { color: 'transparent' } } },
    { type: 'bar', stack: 'w', data: positive, itemStyle: { color: '#005EB8' }, label: { show: true, position: 'top' } },
    { type: 'bar', stack: 'w', data: negative, itemStyle: { color: '#D9534F' }, label: { show: true, position: 'bottom' } }
  ]
};
```

### 折线图 (line)

```javascript
const option = {
  title: { text: chart.title },
  tooltip: { trigger: 'axis' },
  xAxis: { type: 'category', data: chart.data.map(d => d.label) },
  yAxis: { type: 'value' },
  series: [{
    type: 'line',
    data: chart.data.map(d => d.value),
    smooth: true,
    areaStyle: { opacity: 0.15 }
  }]
};
```

### 环形图 (donut)

```javascript
const option = {
  title: { text: chart.title, left: 'center' },
  tooltip: { trigger: 'item' },
  series: [{
    type: 'pie',
    radius: ['40%', '70%'],
    data: chart.data.map(d => ({ name: d.label, value: d.value })),
    label: { formatter: '{b}: {d}%' }
  }]
};
```

---

## 6. 图表容器规范

```html
<!-- 在 Reveal.js slide 内部 -->
<div id="chart-{slideIndex}" style="width:100%; height:55vh;"></div>

<script>
Reveal.on('slidechanged', event => {
  // 延迟初始化当前 slide 的图表，避免尺寸计算错误
  const chartDom = event.currentSlide.querySelector('[id^="chart-"]');
  if (chartDom && !chartDom._echarts) {
    const chart = echarts.init(chartDom, 'mbb-{brand}');
    chart.setOption(/* option */);
    chartDom._echarts = chart;
    window.addEventListener('resize', () => chart.resize());
  }
});
</script>
```
