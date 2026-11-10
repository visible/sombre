export interface Ray {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

export interface Options {
  count: number;
  width: number;
  height: number;
  inner: number;
  outer: number;
  jitter: number;
}

const round = (value: number) => Math.round(value * 100) / 100;

export const noise = (index: number) => {
  const value = Math.sin(index * 12.9898 + 4.1414) * 43_758.5453;
  return value - Math.floor(value);
};

export const angle = (index: number, count: number) =>
  Math.PI + (Math.PI * (index + 0.5)) / count;

export const heading = (
  center: { x: number; y: number },
  point: { x: number; y: number }
) => {
  const value = Math.atan2(point.y - center.y, point.x - center.x);
  return value < 0 ? value + Math.PI * 2 : value;
};

export const lean = (ray: number, target: number, spread: number) => {
  const distance = ray - target;
  return Math.exp(-(distance * distance) / (2 * spread * spread));
};

export const rays = ({
  count,
  width,
  height,
  inner,
  outer,
  jitter,
}: Options): Ray[] => {
  const x = width / 2;
  return Array.from({ length: count }, (_, index) => {
    const turn = angle(index, count);
    const length = outer * (1 - jitter * noise(index));
    return {
      x1: round(x + inner * Math.cos(turn)),
      x2: round(x + length * Math.cos(turn)),
      y1: round(height + inner * Math.sin(turn)),
      y2: round(height + length * Math.sin(turn)),
    };
  });
};
