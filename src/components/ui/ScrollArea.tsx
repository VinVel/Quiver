import type { PartialOptions } from "overlayscrollbars";
import { useOverlayScrollbars } from "overlayscrollbars-react";
import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  type RefObject,
  type HTMLAttributes,
  type ReactNode,
} from "react";
import { classNames } from "./classNames";

export type ScrollAreaHandle = {
  getScrollElement: () => HTMLElement | null;
};

type ScrollAreaProps = HTMLAttributes<HTMLDivElement> & {
  children: ReactNode;
  contentClassName?: string;
  options?: PartialOptions;
};

type UseScrollAreaOverlayParams = {
  options?: PartialOptions;
  rootRef: RefObject<HTMLElement | null>;
  viewportRef: RefObject<HTMLElement | null>;
};

export const defaultScrollAreaOptions: PartialOptions = {
  scrollbars: {
    autoHide: "never",
    clickScroll: true,
    dragScroll: true,
    theme: "os-theme-hyperion",
    visibility: "auto",
  },
};

export function useScrollAreaOverlay({
  options,
  rootRef,
  viewportRef,
}: UseScrollAreaOverlayParams) {
  const [initialize, getOverlayScrollbars] = useOverlayScrollbars({
    options: options ?? defaultScrollAreaOptions,
    defer: true,
  });

  useEffect(() => {
    const rootElement = rootRef.current;
    const viewportElement = viewportRef.current;
    if (!rootElement || !viewportElement) {
      return;
    }

    initialize({
      target: rootElement,
      elements: {
        viewport: viewportElement,
        content: viewportElement,
      },
    });

    return () => getOverlayScrollbars()?.destroy();
  }, [getOverlayScrollbars, initialize, rootRef, viewportRef]);

  return {
    getScrollElement: () =>
      getOverlayScrollbars()?.elements().viewport ?? viewportRef.current,
  };
}

export const ScrollArea = forwardRef<ScrollAreaHandle, ScrollAreaProps>(
  function ScrollArea(
    { className, children, contentClassName, options, ...props },
    ref,
  ) {
    const rootRef = useRef<HTMLDivElement | null>(null);
    const viewportRef = useRef<HTMLDivElement | null>(null);
    const scrollAreaOverlay = useScrollAreaOverlay({
      options,
      rootRef,
      viewportRef,
    });

    useImperativeHandle(
      ref,
      () => ({
        getScrollElement: scrollAreaOverlay.getScrollElement,
      }),
      [scrollAreaOverlay.getScrollElement],
    );

    return (
      <div
        className={classNames(
          "ui-scroll-area",
          "ui-scroll-area--custom",
          className,
        )}
        data-overlayscrollbars-initialize=""
        ref={rootRef}
        {...props}
      >
        <div
          className={classNames("ui-scroll-area__viewport", contentClassName)}
          data-overlayscrollbars-contents=""
          ref={viewportRef}
        >
          {children}
        </div>
      </div>
    );
  },
);
