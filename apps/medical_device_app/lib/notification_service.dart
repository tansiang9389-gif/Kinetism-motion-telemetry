import 'package:flutter_local_notifications/flutter_local_notifications.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

class NotificationService {
  static final FlutterLocalNotificationsPlugin _notificationsPlugin =
      FlutterLocalNotificationsPlugin();
  static bool _initialized = false;

  static Future<void> init() async {
    if (kIsWeb || defaultTargetPlatform != TargetPlatform.android) {
      throw UnsupportedError(
        'Daily reminders are available in the Android app.',
      );
    }
    if (_initialized) return;
    const AndroidInitializationSettings androidSettings =
        AndroidInitializationSettings('ic_stat_kinetism');
    const InitializationSettings initSettings = InitializationSettings(
      android: androidSettings,
    );

    // THE FIX: The label is strictly just "settings:"
    await _notificationsPlugin.initialize(settings: initSettings);
    _initialized = true;
  }

  static Future<void> enableDailyReminder() async {
    await init();
    final android = _notificationsPlugin
        .resolvePlatformSpecificImplementation<
          AndroidFlutterLocalNotificationsPlugin
        >();
    if (android == null) {
      throw UnsupportedError('Android notifications are unavailable.');
    }
    if (await android.requestNotificationsPermission() == false) {
      throw PlatformException(
        code: 'notification-permission-denied',
        message: 'Enable notification permission in phone settings.',
      );
    }
    const AndroidNotificationDetails androidDetails =
        AndroidNotificationDetails(
          'idp_clinic_channel',
          'Daily Exercises',
          channelDescription: 'Reminders to complete your physiotherapy',
          importance: Importance.max,
          priority: Priority.high,
        );
    const NotificationDetails platformDetails = NotificationDetails(
      android: androidDetails,
    );

    await _notificationsPlugin.periodicallyShow(
      id: 0,
      title: 'Kinetism Reminder',
      body: 'Time to complete your daily physiotherapy exercises!',
      repeatInterval: RepeatInterval.daily,
      notificationDetails: platformDetails,
      androidScheduleMode: AndroidScheduleMode.inexactAllowWhileIdle,
    );
  }

  static Future<void> cancelReminders() async {
    await init();
    await _notificationsPlugin.cancel(id: 0);
  }

  static Future<bool> isDailyReminderEnabled() async {
    await init();
    final pending = await _notificationsPlugin.pendingNotificationRequests();
    return pending.any((notification) => notification.id == 0);
  }
}
