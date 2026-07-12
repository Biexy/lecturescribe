import { ActivityBar } from "./components/ActivityBar";
import { Header } from "./components/Header";
import { QueueWorkspace } from "./components/QueueWorkspace";
import { SourcePanel } from "./components/SourcePanel";
import { AboutModal, ItemDrawer, SummaryModal } from "./components/dialogs/ProductDialogs";
import { PasteLinksModal } from "./components/dialogs/PasteLinksModal";
import { PreflightModal } from "./components/dialogs/PreflightModal";
import { SettingsModal } from "./components/dialogs/SettingsModal";
import { SetupCenter } from "./components/dialogs/SetupCenter";
import { ToastRegion } from "./components/ui";
import { useAppController } from "./hooks/useAppController";
import { capabilityForSelection } from "./lib/setup";
import { itemSnapshots, selectedItemIds } from "./state/app-state";

export function App() {
  const controller = useAppController();
  const {
    state,
    dispatch,
    pastedText,
    setPastedText,
    starting,
    settingsSaving,
    setupBusy,
    setupTest,
    setupError,
    modelOptions,
    modelValidation,
    modelBusy,
    actions,
  } = controller;

  if (state.booting) {
    return (
      <div className="loading-screen">
        <div className="brand-mark"><span className="spinner large" /></div>
        <h1>LectureScribe</h1>
        <p>Opening the local run ledger and checking setup...</p>
      </div>
    );
  }

  const detailItem = state.preview?.items.find((item) => item.id === state.detailItemId) ?? null;
  const detailSnapshot = detailItem ? itemSnapshots(state.job).get(detailItem.id) : undefined;
  const selectedIds = new Set(selectedItemIds(state));
  const selectedItems = state.preview?.items.filter((item) => selectedIds.has(item.id)) ?? [];
  const setupCapability = capabilityForSelection(state.mode, selectedItems);

  return (
    <div className="app-shell">
      <Header
        capability={setupCapability}
        environment={state.environment}
        onAbout={() => dispatch({ type: "dialog", dialog: "about" })}
        onOpenOutput={() => void actions.openOutput()}
        onSettings={() => dispatch({ type: "dialog", dialog: "settings" })}
        onSetup={() => dispatch({ type: "dialog", dialog: "setup" })}
        onTheme={actions.setTheme}
        settings={state.settings}
      />

      <div className="app-workspace">
        <SourcePanel
          onAddFolder={() => void actions.addFolder()}
          onAddMedia={() => void actions.addMedia()}
          onAddText={() => void actions.addTextFiles()}
          onClear={actions.clearSources}
          onMode={actions.setMode}
          onPaste={() => dispatch({ type: "dialog", dialog: "sources" })}
          onRefresh={actions.refreshPreview}
          onRemove={actions.removeSource}
          onReview={() => void actions.reviewPlan()}
          state={state}
        />
        <QueueWorkspace
          onDetail={(id) => dispatch({ type: "detail", id })}
          onFilter={actions.setFilter}
          onOpenArtifact={(itemId, kind, reveal) => void actions.openArtifact(itemId, kind, reveal)}
          onOpenHistory={(entry) => void actions.openHistory(entry)}
          onRefresh={actions.refreshPreview}
          onSearch={actions.setSearch}
          onSelect={actions.selectItems}
          onToggle={actions.toggleItem}
          onWorkspace={actions.setWorkspace}
          state={state}
        />
      </div>

      <ActivityBar
        onCancel={() => void actions.cancelJob()}
        onExpand={(expanded) => dispatch({ type: "activity", expanded })}
        onOpenOutput={() => void actions.openOutput(state.job?.summary ? state.job.id : undefined)}
        onPause={() => void actions.pauseJob()}
        onResume={() => void actions.resumeJob()}
        onRetry={() => void actions.retryFailed()}
        state={state}
      />

      <PasteLinksModal
        onAdd={actions.addPastedLinks}
        onChange={setPastedText}
        onClose={() => dispatch({ type: "dialog", dialog: null })}
        open={state.dialog === "sources"}
        value={pastedText}
      />

      <PreflightModal
        modelOptions={modelOptions}
        onClose={actions.closeSetup}
        onFixSetup={() => dispatch({ type: "dialog", dialog: "setup" })}
        onRebuild={(overrides) => void actions.reviewPlan(overrides)}
        onStart={() => void actions.startPlan()}
        open={state.dialog === "preflight"}
        plan={state.plan}
        rebuilding={state.planLoading}
        starting={starting}
      />

      <SetupCenter
        busy={setupBusy}
        environment={state.environment}
        focus={setupCapability}
        onCheck={() => void actions.refreshEnvironment()}
        onChooseFfmpeg={() => void actions.chooseFfmpeg()}
        onChooseOutput={() => void actions.chooseOutput()}
        onClose={() => dispatch({ type: "dialog", dialog: null })}
        onDeleteKey={() => void actions.deleteApiKey()}
        onExportDiagnostics={() => void actions.exportDiagnostics()}
        onInstallDownloader={() => void actions.installDownloader()}
        onOpenLink={(target) => void actions.openLink(target)}
        onSaveKey={(key) => void actions.saveApiKey(key)}
        onTest={() => void actions.runSetupTest()}
        open={state.dialog === "setup"}
        settings={state.settings}
        testResult={setupTest}
        testError={setupError}
      />

      <SettingsModal
        environment={state.environment}
        modelBusy={modelBusy}
        modelOptions={modelOptions}
        modelValidation={modelValidation}
        onChooseOutput={() => void actions.chooseOutput()}
        onClose={() => dispatch({ type: "dialog", dialog: null })}
        onExportDiagnostics={() => void actions.exportDiagnostics()}
        onOpenDiagnostics={() => dispatch({ type: "dialog", dialog: "setup" })}
        onSave={(settings) => void actions.saveSettings(settings)}
        onValidateModel={(model) => void actions.validateTranscriptionModel(model)}
        open={state.dialog === "settings"}
        saving={settingsSaving}
        settings={state.settings}
      />

      <AboutModal
        environment={state.environment}
        onClose={() => dispatch({ type: "dialog", dialog: null })}
        onGitHub={() => void actions.openLink("github")}
        open={state.dialog === "about"}
      />

      <SummaryModal
        job={state.job}
        onClose={() => dispatch({ type: "dialog", dialog: null })}
        onOpenOutput={() => void actions.openOutput(state.job?.id)}
        onRetry={() => void actions.retryFailed()}
        open={state.dialog === "summary"}
      />

      <ItemDrawer
        item={detailItem}
        onClose={() => dispatch({ type: "detail", id: null })}
        onOpenArtifact={(itemId, kind, reveal) => void actions.openArtifact(itemId, kind, reveal)}
        open={Boolean(detailItem)}
        settings={state.settings}
        snapshot={detailSnapshot}
      />

      <ToastRegion
        onAction={actions.handleToastAction}
        onDismiss={(id) => dispatch({ type: "dismiss_toast", id })}
        toasts={state.toasts}
      />
    </div>
  );
}
