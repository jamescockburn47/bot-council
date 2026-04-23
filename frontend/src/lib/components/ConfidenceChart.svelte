<script lang="ts">
  import {
    Chart,
    CategoryScale,
    LinearScale,
    LineController,
    PointElement,
    LineElement,
    Tooltip,
    Legend,
  } from 'chart.js';
  import { AGENT_COLORS } from '$lib/utils/agent-colors';

  // Chart.js v4 requires the controller for the chart type to be registered
  // alongside the scales/elements. Without LineController a `type: 'line'`
  // Chart() throws "line is not a registered controller."
  Chart.register(
    CategoryScale,
    LinearScale,
    LineController,
    PointElement,
    LineElement,
    Tooltip,
    Legend,
  );

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
            grid: { color: 'rgba(31,31,47,0.8)' },
            ticks: { color: '#8888A0', font: { family: 'monospace', size: 11 } },
          },
          y: {
            min: 0,
            max: 100,
            grid: { color: 'rgba(31,31,47,0.8)' },
            ticks: { color: '#8888A0', font: { family: 'monospace', size: 11 } },
          },
        },
        plugins: {
          legend: {
            labels: {
              color: '#8888A0',
              font: { family: 'monospace', size: 11 },
              usePointStyle: true,
              pointStyle: 'circle',
            },
          },
          tooltip: {
            backgroundColor: '#0F0F17',
            titleColor: '#E4E4EF',
            bodyColor: '#8888A0',
            borderColor: 'rgba(31,31,47,0.8)',
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

<div class="card-term">
  <p class="tm-eyebrow" style="color: var(--indigo-400); margin-bottom: 12px;">Confidence Trajectories</p>
  <div class="h-48">
    <canvas bind:this={canvas}></canvas>
  </div>
</div>
