import { useLayoutEffect, useMemo, useRef, useState } from "react";

export interface VirtualRow {
  index: number;
  start: number;
  size: number;
}

export function useVirtualRows(count: number, rowHeight = 48, overscan = 6) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(400);

  useLayoutEffect(() => {
    const element = scrollRef.current;
    if (!element) return;
    const update = () => setViewportHeight(element.clientHeight || 400);
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const rows = useMemo(() => {
    if (count === 0) return [];
    const start = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan);
    const end = Math.min(
      count,
      Math.ceil((scrollTop + viewportHeight) / rowHeight) + overscan,
    );
    const result: VirtualRow[] = [];
    for (let index = start; index < end; index += 1) {
      result.push({ index, start: index * rowHeight, size: rowHeight });
    }
    return result;
  }, [count, overscan, rowHeight, scrollTop, viewportHeight]);

  return {
    scrollRef,
    rows,
    totalHeight: count * rowHeight,
    onScroll(event: Event) {
      setScrollTop((event.currentTarget as HTMLDivElement).scrollTop);
    },
  };
}
