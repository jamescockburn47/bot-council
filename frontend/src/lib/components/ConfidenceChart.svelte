<script lang="ts">
  import {
    Chart,
    CategoryScale,
    LinearScale,
    PointElement,
    LineElement,
    Tooltip,
    Legend,
  } from 'chart.js';
  import { AGENT_COLORS } from '$lib/utils/agent-colors';

  Chart.register(CategoryScale, LinearScale, PointElement, LineElement, Tooltip, Legend);

  let { trajectories }: { trajectories: Record<string, (number | null)[]> } = $props();
  let canvas: HTMLCanvasElement;
  let chart: Chart | undefined;

  $effect(() => {
    if (chart) chart.destroy();
    if (!canvas) return;

    const labels = ['R1', 'R2', 'R3', 'R4'];
    const datasets = Object.entries(trajectories).map(([agent, values]) => ({
      label: agent,
      data: values.slice(1),
      borderColor: AGENT_COLORS[agent] ?? '#94a3b8',
      backgroundColor: (AGENT_COLORS[agent] ?? '#94a3b8') + '20',
      borderWidth: 2,
      pointRadius: 4,
      pointHoverRadius: 6,
      tension: 0.3,
    }));

    chart = new Chart(canvas, {
      type: 'line',
      data: { labels, datasets },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        scales: {
          x: {
            grid: { color: '#1e1e3a' },
            ticks: { color: '#4a4a6a', font: { family: 'monospace', size: 11 } },
          },
          y: {
            min: 0,
            max: 100,
            grid: { color: '#1e1e3a' },
            ticks: { color: '#4a4a6a', font: { family: 'monospace', size: 11 } },
          },
        },
        plugins: {
          legend: {
            labels: {
              color: '#94a3b8',
              font: { family: 'monospace', size: 11 },
              usePointStyle: true,
              pointStyle: 'circle',
            },
          },
          tooltip: {
            backgroundColor: '#0a0a1a',
            titleColor: '#e2e8f0',
            bodyColor: '#94a3b8',
            borderColor: '#1e1e3a',
            borderWidth: 1,
          },
        },
      },
    });

    return () => {
      chart?.destroy();
      chart = undefined;
    };
  });
</script>

<div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-4">
  <h3 class="text-xs mono text-[var(--text-muted)] mb-3 uppercase tracking-wider">
    Confidence Trajectories
  </h3>
  <div class="h-48">
    <canvas bind:this={canvas}></canvas>
  </div>
</div>
