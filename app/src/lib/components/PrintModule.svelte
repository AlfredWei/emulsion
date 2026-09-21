<script>
  import PrintLayoutView from "$lib/components/PrintLayoutView.svelte";
  import { print } from "$lib/state/print.svelte.js";
  import PrintPanel from "$lib/components/PrintPanel.svelte";
  import { handleChoosePrintCustomProfile, handlePrint, handleExportPdf } from "$lib/actions/printActions.js";
</script>

<div class="print-body">
  <PrintLayoutView
    items={print.items}
    template={print.template}
    fitMode={print.fitMode}
    rows={print.rows}
    cols={print.cols}
    cellSpacing={print.cellSpacing}
    paperSize={print.paperSize}
    orientation={print.orientation}
    margins={print.margins}
    colorManagement={print.colorManagementSettings}
    printReadyUrls={print.readyUrls}
  />
  <PrintPanel
    itemCount={print.items.length}
    template={print.template}
    onTemplateChange={(v) => (print.template = v)}
    fitMode={print.fitMode}
    onFitModeChange={(v) => (print.fitMode = v)}
    rows={print.rows}
    cols={print.cols}
    onRowsChange={(v) => (print.rows = v)}
    onColsChange={(v) => (print.cols = v)}
    cellSpacing={print.cellSpacing}
    onCellSpacingChange={(v) => (print.cellSpacing = v)}
    paperSize={print.paperSize}
    onPaperSizeChange={(v) => (print.paperSize = v)}
    orientation={print.orientation}
    onOrientationChange={(v) => (print.orientation = v)}
    margins={print.margins}
    onMarginChange={(side, v) => (print.margins = { ...print.margins, [side]: v })}
    colorManaged={print.colorManaged}
    onColorManagedChange={(v) => (print.colorManaged = v)}
    profileTarget={print.profileTarget}
    onProfileTargetChange={(v) => (print.profileTarget = /** @type {typeof print.profileTarget} */ (v))}
    customProfilePath={print.customProfilePath}
    onChooseCustomProfile={handleChoosePrintCustomProfile}
    intent={print.intent}
    onIntentChange={(v) => (print.intent = /** @type {typeof print.intent} */ (v))}
    printing={print.printing}
    onPrint={handlePrint}
    exportingPdf={print.exportingPdf}
    onExportPdf={handleExportPdf}
  />
</div>

<style>
  .print-body {
    flex: 1;
    display: flex;
    min-height: 0;
    position: relative;
  }
</style>
