import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from '../i18n';
import { Calibration } from './Calibration';
import type { AppConfig } from '../bindings/AppConfig';
import type { CameraDevice } from '../bindings/CameraDevice';

export function Settings({ onBack }: { onBack: () => void }) {
  const { t, lang, setLang } = useI18n();
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [cameras, setCameras] = useState<CameraDevice[]>([]);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [calibrating, setCalibrating] = useState(false);
  const [calibrationMessage, setCalibrationMessage] = useState<string | null>(null);
  const pendingLang = useRef<string | null>(null);

  useEffect(() => {
    invoke<AppConfig>('get_config').then(setConfig).catch(console.error);
    invoke<CameraDevice[]>('list_cameras')
      .then(setCameras)
      .catch((e) => {
        console.error('枚举摄像头失败:', e);
        setCameras([]);
      });
  }, []);

  const update = useCallback(
    <K extends keyof AppConfig>(key: K, value: AppConfig[K]) => {
      setConfig((prev) => (prev ? { ...prev, [key]: value } : prev));
    },
    [],
  );

  const handleSave = useCallback(async () => {
    if (!config) return;
    setSaving(true);
    try {
      // 保存时同时设置语言（更新 Rust 端 + 托盘菜单）
      const toSave = { ...config, language: lang };
      await invoke('set_config', { newConfig: toSave });
      setSaved(true);
      setTimeout(() => setSaved(false), 1500);
    } catch (e) {
      console.error('保存设置失败:', e);
    } finally {
      setSaving(false);
    }
  }, [config, lang]);

  const handleLangChange = useCallback(
    (newLang: string) => {
      setLang(newLang as 'zh-CN' | 'en');
      pendingLang.current = newLang;
    },
    [setLang],
  );

  const handleCalibrationStart = useCallback(() => {
    setCalibrating(true);
    setCalibrationMessage(null);
    invoke('start_calibration').catch(console.error);
  }, []);

  const handleCalibrationComplete = useCallback(
    (yaw: number, pitch: number) => {
      setCalibrating(false);
      setCalibrationMessage(t('calibration.success'));
      // 更新本地 config 显示
      setConfig((prev) =>
        prev ? { ...prev, neutral_yaw: yaw, neutral_pitch: pitch } : prev,
      );
      // 立即保存校准结果
      if (config) {
        const updated = { ...config, neutral_yaw: yaw, neutral_pitch: pitch, language: lang };
        invoke('set_config', { newConfig: updated }).catch(console.error);
      }
    },
    [config, lang, t],
  );

  const handleCalibrationFail = useCallback(() => {
    setCalibrating(false);
    setCalibrationMessage(t('calibration.no_face'));
  }, [t]);

  const handleCalibrationCancel = useCallback(() => {
    setCalibrating(false);
    invoke('cancel_calibration').catch(console.error);
  }, []);

  if (!config) {
    return <div className="settings-loading">Loading…</div>;
  }

  return (
    <div className="settings">
      <h2>{t('settings.title')}</h2>

      {/* 阈值 */}
      <fieldset className="settings-group">
        <label>
          {t('settings.yaw_threshold')}
          <div className="slider-row">
            <input
              type="range"
              min={1}
              max={30}
              step={0.5}
              value={config.yaw_threshold}
              onChange={(e) => update('yaw_threshold', Number(e.target.value))}
            />
            <span className="slider-value">{config.yaw_threshold.toFixed(1)}°</span>
          </div>
        </label>

        <label>
          {t('settings.pitch_threshold')}
          <div className="slider-row">
            <input
              type="range"
              min={1}
              max={30}
              step={0.5}
              value={config.pitch_threshold}
              onChange={(e) => update('pitch_threshold', Number(e.target.value))}
            />
            <span className="slider-value">{config.pitch_threshold.toFixed(1)}°</span>
          </div>
        </label>
      </fieldset>

      {/* 校准 */}
      <fieldset className="settings-group">
        <legend>{t('settings.calibrate')}</legend>
        {calibrationMessage && (
          <div
            className={`calibration-message ${calibrationMessage.includes('失败') || calibrationMessage.includes('failed') ? 'error' : 'success'}`}
          >
            {calibrationMessage}
          </div>
        )}
        {!calibrating ? (
          <div>
            <button onClick={handleCalibrationStart} className="btn-primary">
              {t('settings.calibrate')}
            </button>
            <div className="neutral-display">
              yaw: {config.neutral_yaw.toFixed(1)}° / pitch: {config.neutral_pitch.toFixed(1)}°
            </div>
          </div>
        ) : (
          <Calibration
            onComplete={handleCalibrationComplete}
            onFail={handleCalibrationFail}
            onCancel={handleCalibrationCancel}
          />
        )}
      </fieldset>

      {/* 摄像头 */}
      <fieldset className="settings-group">
        <label>
          {t('settings.camera_index')}
          <select
            value={config.camera_index}
            onChange={(e) => update('camera_index', Number(e.target.value))}
          >
            {cameras.length > 0 ? (
              cameras.map((cam) => (
                <option key={cam.index} value={cam.index}>
                  {cam.name}
                </option>
              ))
            ) : (
              <>
                <option value={0}>{t('settings.camera_0')}</option>
                <option value={1}>{t('settings.camera_1')}</option>
                <option value={2}>{t('settings.camera_2')}</option>
              </>
            )}
          </select>
        </label>
      </fieldset>

      {/* 开关 */}
      <fieldset className="settings-group toggles">
        <label className="toggle-row">
          {t('settings.sound_enabled')}
          <button
            className={`toggle-btn ${config.sound_enabled ? 'on' : 'off'}`}
            onClick={() => update('sound_enabled', !config.sound_enabled)}
          >
            {config.sound_enabled ? 'ON' : 'OFF'}
          </button>
        </label>

        {navigator.platform.toLowerCase().startsWith('win') && (
          <label className="toggle-row">
            {t('settings.autostart_enabled')}
            <button
              className={`toggle-btn ${config.autostart_enabled ? 'on' : 'off'}`}
              onClick={() => update('autostart_enabled', !config.autostart_enabled)}
            >
              {config.autostart_enabled ? 'ON' : 'OFF'}
            </button>
          </label>
        )}
      </fieldset>

      {/* 语言 */}
      <fieldset className="settings-group">
        <label>
          {t('settings.language')}
          <select value={lang} onChange={(e) => handleLangChange(e.target.value)}>
            <option value="zh-CN">中文</option>
            <option value="en">English</option>
          </select>
        </label>
      </fieldset>

      {/* 高级设置 */}
      <div className="settings-group advanced">
        <button
          type="button"
          className="advanced-toggle"
          onClick={() => setShowAdvanced((prev) => !prev)}
        >
          {t('settings.advanced')}
        </button>
        {showAdvanced && (
          <>
            <label>
              {t('settings.streak_threshold')}
              <div className="slider-row">
                <input
                  type="range"
                  min={0}
                  max={30}
                  step={0.1}
                  value={config.off_axis_streak_threshold_seconds}
                  onChange={(e) =>
                    update('off_axis_streak_threshold_seconds', Number(e.target.value))
                  }
                />
                <span className="slider-value">
                  {config.off_axis_streak_threshold_seconds.toFixed(1)}s
                </span>
              </div>
            </label>
            <label>
              {t('settings.repeat_interval')}
              <div className="slider-row">
                <input
                  type="range"
                  min={10}
                  max={120}
                  step={1}
                  value={config.off_axis_repeat_interval_seconds}
                  onChange={(e) =>
                    update('off_axis_repeat_interval_seconds', Number(e.target.value))
                  }
                />
                <span className="slider-value">
                  {config.off_axis_repeat_interval_seconds.toFixed(0)}s
                </span>
              </div>
            </label>
          </>
        )}
      </div>

      {/* 操作按钮 */}
      <div className="settings-actions">
        <button onClick={onBack} className="btn-secondary">
          {t('settings.cancel')}
        </button>
        <button onClick={handleSave} disabled={saving} className="btn-primary">
          {saved ? t('settings.saved') : t('settings.save')}
        </button>
      </div>
    </div>
  );
}
