import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { AppConfig, Profile, Settings } from "../../lib/config";
import { upsertProfile } from "../../lib/config-editing";
import {
  getMouseSettings,
  listenMouseBattery,
  normalizeCommandError,
  setMouseDpi,
  setMousePollRate,
} from "../../lib/backend";
import {
  MOUSE_DPI_MAX,
  MOUSE_DPI_MIN,
  MOUSE_POLL_RATES,
  type MouseSettings as MouseSettingsState,
} from "../../lib/onboard-mapping";
import { PillTrack } from "../PillTrack";
import { Notice, Toggle } from "../shared";

export interface MouseSettingsProps {
  activeConfig: AppConfig;
  /** The profile being edited (selected in the sidebar). */
  activeProfile: Profile | null;
  /** The profile the runtime applies now: its edits reach the mouse at once. */
  runtimeProfileId: string | null;
  updateDraft: (updater: (config: AppConfig) => AppConfig) => void;
  updateSettings: (patch: Partial<Settings>) => void;
}

/** Mouse tab: battery (plus the tray badge switch) and the edited profile's own
 *  DPI / poll rate. The runtime applies a profile's values on every switch to
 *  it; the mouse stays as it is for a profile without them. */
export function MouseSettings({
  activeConfig,
  activeProfile,
  runtimeProfileId,
  updateDraft,
  updateSettings,
}: MouseSettingsProps) {
  const { t } = useTranslation();
  const [device, setDevice] = useState<MouseSettingsState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [dpiDraft, setDpiDraft] = useState("");

  async function readDevice() {
    setBusy(true);
    try {
      setDevice(await getMouseSettings());
      setError(null);
    } catch (unknownError) {
      setError(normalizeCommandError(unknownError).message);
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    void readDevice();
    let unlisten: (() => void) | undefined;
    void listenMouseBattery(({ percent }) => {
      setDevice((current) => (current ? { ...current, batteryPercent: percent } : current));
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const profileDpi = activeProfile?.mouseDpi;
  const profilePoll = activeProfile?.mousePollRateHz;
  const overrides = profileDpi !== undefined || profilePoll !== undefined;
  const appliesNow = activeProfile !== null && activeProfile.id === runtimeProfileId;

  useEffect(() => {
    setDpiDraft(profileDpi !== undefined ? String(profileDpi) : "");
  }, [profileDpi, activeProfile?.id]);

  function saveProfile(patch: Pick<Profile, "mouseDpi" | "mousePollRateHz">) {
    if (!activeProfile) return;
    updateDraft((config) => upsertProfile(config, { ...activeProfile, ...patch }));
  }

  /** Writes a value to the mouse when the edited profile is the one in effect. */
  async function applyNow(request: () => Promise<MouseSettingsState>) {
    if (!appliesNow) return;
    setBusy(true);
    try {
      setDevice(await request());
      setError(null);
    } catch (unknownError) {
      setError(normalizeCommandError(unknownError).message);
    } finally {
      setBusy(false);
    }
  }

  const dpiValue = Number(dpiDraft);
  const dpiValid =
    Number.isInteger(dpiValue) && dpiValue >= MOUSE_DPI_MIN && dpiValue <= MOUSE_DPI_MAX;

  return (
    <>
      <section className="settings-section">
        <div className="settings-section__header">
          <span className="settings-section__title">{t("mouse.batteryHeader")}</span>
          <div className="settings-section__master">
            <span className="settings-section__master-label">{t("mouse.batteryInTray")}</span>
            <Toggle
              checked={activeConfig.settings.showBatteryInTray ?? false}
              onChange={(checked) => updateSettings({ showBatteryInTray: checked || undefined })}
              ariaLabel={t("mouse.batteryInTray")}
            />
          </div>
        </div>
        <p className="panel__muted help-sm">{t("mouse.batteryHelp")}</p>
        {device ? (
          <p className="mt-8">
            <strong>{t("mouse.batteryValue", { percent: device.batteryPercent })}</strong>
          </p>
        ) : null}
        {error ? (
          <Notice variant="warning" className="mt-12">
            <p>{t("mouse.unavailable")}</p>
            <p className="panel__muted help-caption">{error}</p>
            <button
              type="button"
              className="action-button action-button--secondary action-button--small mt-8"
              onClick={() => void readDevice()}
              disabled={busy}
            >
              {t("mouse.retry")}
            </button>
          </Notice>
        ) : null}
      </section>

      {activeProfile ? (
        <section className="settings-section">
          <div className="settings-section__header">
            <span className="settings-section__title">
              {t("mouse.profileHeader", { name: activeProfile.name })}
            </span>
            <div className="settings-section__master">
              <span className="settings-section__master-label">{t("mouse.profileOverride")}</span>
              <Toggle
                checked={overrides}
                onChange={(checked) =>
                  saveProfile(
                    checked
                      ? { mouseDpi: device?.dpi ?? 1600, mousePollRateHz: device?.pollRateHz ?? 1000 }
                      : { mouseDpi: undefined, mousePollRateHz: undefined },
                  )
                }
                ariaLabel={t("mouse.profileOverride")}
              />
            </div>
          </div>
          <p className="panel__muted help-sm">
            {overrides ? t("mouse.profileHelpOn") : t("mouse.profileHelpOff")}
          </p>

          {overrides ? (
            <>
              <div className="osd-settings-row mt-12">
                <span className="osd-settings-row__label">{t("mouse.pollRate")}</span>
                <p className="panel__muted help-caption">{t("mouse.pollRateCaption")}</p>
                <PillTrack
                  items={MOUSE_POLL_RATES.map((hz) => ({
                    key: String(hz),
                    label: t("mouse.pollRateOption", { hz }),
                  }))}
                  active={String(profilePoll ?? "")}
                  onSelect={(key) => {
                    const hz = Number(key);
                    saveProfile({ mouseDpi: profileDpi, mousePollRateHz: hz });
                    void applyNow(() => setMousePollRate(hz));
                  }}
                />
              </div>

              <div className="osd-settings-row mt-12">
                <label className="field">
                  <span className="field__label">{t("mouse.dpi")}</span>
                  <div className="input-suffix">
                    <input
                      type="number"
                      min={MOUSE_DPI_MIN}
                      max={MOUSE_DPI_MAX}
                      step={50}
                      value={dpiDraft}
                      onChange={(e) => setDpiDraft(e.target.value)}
                    />
                    <span className="input-suffix__unit">{t("mouse.dpiUnit")}</span>
                  </div>
                </label>
                <p className="panel__muted help-caption">
                  {t("mouse.dpiCaption", { min: MOUSE_DPI_MIN, max: MOUSE_DPI_MAX })}
                </p>
                <button
                  type="button"
                  className="action-button action-button--secondary action-button--small mt-8"
                  onClick={() => {
                    saveProfile({ mouseDpi: dpiValue, mousePollRateHz: profilePoll });
                    void applyNow(() => setMouseDpi(dpiValue));
                  }}
                  disabled={busy || !dpiValid || dpiValue === profileDpi}
                >
                  {t("mouse.apply")}
                </button>
              </div>
            </>
          ) : null}
        </section>
      ) : null}
    </>
  );
}
