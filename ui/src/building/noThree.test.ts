// Vỏ DOM và mọi module thuần của cảnh Tòa nhà không được import three (docs/OFFICE_BUILDING.md §9.4): three chỉ nằm trong
// `three/*`, nạp bằng `lazy(() => import(...))` nên không vào bundle chính của popup.
import { describe, expect, it } from 'vitest';

const FILES = import.meta.glob<string>(
  ['./BuildingScene.tsx', './{floors,layout,seating,figure,secretary,route,camera,loop,model,useSecretary}.ts', '../sceneBrief.ts', '../useSceneBrief.ts', '../components/scenes.tsx', '../components/SceneBoundary.tsx', '../webgl.ts'],
  { query: '?raw', import: 'default', eager: true },
);

describe('no three in the main bundle', () => {
  it('scans_the_expected_files', () => {
    expect(Object.keys(FILES).length).toBe(16);
  });

  it('none_of_them_import_three', () => {
    for (const [file, src] of Object.entries(FILES)) {
      expect(src, `${file} import three`).not.toMatch(/from\s+['"]three/);
      expect(src, `${file} import() three`).not.toMatch(/import\(\s*['"]three/);
      expect(src, `${file} requires three/ directory`).not.toMatch(/from\s+['"]\.\/three\//);
    }
  });

  it('the_canvas_is_only_reached_through_a_lazy_import', () => {
    const shell = FILES['./BuildingScene.tsx'] ?? '';
    expect(shell).toMatch(/lazy\(\(\) => import\('\.\/three\/BuildingCanvas'\)\)/);
  });
});
