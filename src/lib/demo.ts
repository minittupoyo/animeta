import {
  defaultSettings,
  type ImportedFile,
  type Episode,
  type Work,
  type PlanRequest,
  type Preview,
  type PreviewRow,
} from './types';
export const demoWork: Work = {
  annictId: 1,
  title: '星をつなぐ旅',
  media: 'TV',
  episodesCount: 12,
  noEpisodes: false,
  seasonYear: 2026,
  seasonName: 'AUTUMN',
};
export const demoEpisodes: Episode[] = [
  '旅のはじまり',
  '夜空の約束',
  '見知らぬ街',
  '星降る丘',
  '遠くの灯り',
  '帰り道',
].map((title, i) => ({
  annictId: i + 1,
  number: i + 1,
  numberText: `第${i + 1}話`,
  sortNumber: i + 1,
  title,
}));
export const demoFiles: ImportedFile[] = [1, 2, 3, 4].map((n) => ({
  id: `demo-${n}`,
  path: `/サンプル/録画/星をつなぐ旅 第0${n}話.mkv`,
  name: `星をつなぐ旅 第0${n}話.mkv`,
  size: 684_195_840 + n * 35_700_000,
  modifiedMs: Date.now(),
  guessedNumber: n,
  error: null,
}));
export const demoSettings = () => ({
  ...defaultSettings(),
  outputDir: '/サンプル/整理済み',
});
// Browser-only illustration. Production naming and validation always run in Rust.
export function demoPreview(request: PlanRequest): Preview {
  return {
    planId: 'browser-sample',
    rows: request.assignments.map((a): PreviewRow => {
      const file = demoFiles.find((f) => f.id === a.fileId)!;
      const episode = a.workOnly
        ? undefined
        : demoEpisodes.find((e) => e.annictId === a.episodeId);
      const n = a.manualNumber ?? episode?.number;
      const label =
        a.manualLabel ?? (n != null ? `第${String(n).padStart(2, '0')}話` : '');
      const title = a.manualTitle ?? episode?.title ?? '';
      const values: Record<string, string> = {
        work_title: request.work.title,
        episode_label: label,
        episode_title: title,
        episode_number: n == null ? '' : String(n),
        original_stem: file.name.replace(/\.[^.]+$/, ''),
        annict_work_id: String(request.work.annictId),
        annict_episode_id: episode ? String(episode.annictId) : '',
      };
      const replace = (text: string) =>
        text.replace(
          /\{(\w+)(?::0([2-9]))?\}/g,
          (_, key: string, width: string) =>
            width
              ? (values[key] || '').padStart(Number(width), '0')
              : values[key] || '',
        );
      const stem = replace(
        request.settings.template.replace(
          /\[([^\[\]]*)\]/g,
          (_, part: string) =>
            [...part.matchAll(/\{(\w+)(?::0[2-9])?\}/g)].every(
              (m) => values[m[1]!] !== '',
            )
              ? replace(part)
              : '',
        ),
      ).replace(/[<>:"/\\|?*]/g, '_');
      const outputName = request.settings.rename ? `${stem}.mkv` : file.name;
      return {
        fileId: file.id,
        inputName: file.name,
        inputPath: file.path,
        outputName,
        outputPath:
          request.settings.outputMode === 'replace'
            ? `${file.path.slice(0, file.path.lastIndexOf('/'))}/${outputName}`
            : `${request.settings.outputDir}/${outputName}`,
        tags: request.settings.embed
          ? {
              title:
                [label, title].filter(Boolean).join(' ') || request.work.title,
              show: request.work.title,
              episode_id: label,
              description: title,
              comment: `https://annict.com/works/${request.work.annictId}`,
              episode_sort: n == null ? '' : String(n),
            }
          : {},
        errors: !a.workOnly && !episode ? ['エピソードを選択してください'] : [],
        enabled: a.enabled,
        episodeId: episode?.annictId ?? null,
      };
    }),
  };
}
