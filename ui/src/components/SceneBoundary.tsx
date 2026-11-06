// Error boundary cho cảnh Văn phòng: cảnh lỗi (render, nạp chunk) thì hiện `fallback`, không làm trống cả cửa sổ. Hàng đợi
// duyệt nằm ngoài cảnh nên duyệt không bao giờ phụ thuộc cảnh (docs/OFFICE_BUILDING.md §10).
import { Component, type ErrorInfo, type ReactNode } from 'react';

export interface SceneBoundaryProps {
  fallback: ReactNode;
  /** Báo lỗi cho chủ (ví dụ để nhớ lỗi trong phiên). */
  onError?: (error: Error) => void;
  children: ReactNode;
}

interface SceneBoundaryState {
  failed: boolean;
}

export class SceneBoundary extends Component<SceneBoundaryProps, SceneBoundaryState> {
  override state: SceneBoundaryState = { failed: false };

  static getDerivedStateFromError(): SceneBoundaryState {
    return { failed: true };
  }

  override componentDidCatch(error: Error, _info: ErrorInfo): void {
    this.props.onError?.(error);
  }

  override render(): ReactNode {
    return this.state.failed ? this.props.fallback : this.props.children;
  }
}
