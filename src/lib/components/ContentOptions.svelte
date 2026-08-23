<script lang="ts">
  import type { DownloadContentSelection } from "../types";
  import { t } from "../i18n";

  let {
    options,
    onChange,
    label = "下载内容"
  }: {
    options: DownloadContentSelection;
    onChange: (options: DownloadContentSelection) => void;
    label?: string;
  } = $props();

  function setOption(key: keyof DownloadContentSelection, checked: boolean) {
    onChange({ ...options, [key]: checked });
  }
</script>

<fieldset class="content-options" aria-label={label}>
  <legend>{label}</legend>
  <label><input type="checkbox" checked={options.downloadVideo} onchange={(event) => setOption("downloadVideo", event.currentTarget.checked)} />{$t("content.video")}</label>
  <label><input type="checkbox" checked={options.downloadAudio} onchange={(event) => setOption("downloadAudio", event.currentTarget.checked)} />{$t("content.audio")}</label>
  <label><input type="checkbox" checked={options.downloadCover} onchange={(event) => setOption("downloadCover", event.currentTarget.checked)} />{$t("content.cover")}</label>
  <label><input type="checkbox" checked={options.downloadCaption} onchange={(event) => setOption("downloadCaption", event.currentTarget.checked)} />{$t("content.caption")}</label>
</fieldset>
