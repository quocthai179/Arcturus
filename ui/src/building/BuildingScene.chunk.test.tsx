// Chunk three nạp lỗi (import() bị từ chối, không phải ném đồng bộ): cảnh phải lùi về `live`.
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('./three/BuildingCanvas', () => {
  throw new Error('chunk failed to load');
});

import { agent } from '../test/fixtures';
import { BuildingScene } from './BuildingScene';

afterEach(cleanup);

describe('BuildingScene with a rejected lazy chunk', () => {
  it('falls_back_to_the_live_office', async () => {
    const spy = vi.spyOn(console, 'error').mockImplementation(() => {});
    render(<BuildingScene desks={[agent({ id: 'agt_a', name: 'Khoa', mood: 'working' })]} support={[]} canSelect={() => false} onSelect={() => {}} motion={false} webgl />);
    await waitFor(() => expect(screen.getByText('Máy này không dựng được cảnh 3D, đang hiện văn phòng 2D.')).toBeTruthy());
    expect(screen.getByRole('region', { name: 'Sàn văn phòng' })).toBeTruthy();
    spy.mockRestore();
  });
});
