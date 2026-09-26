"""Google's published API limits, as an independent reference for GUI sync checks.

The navigation benchmark retains its original explicit budgets. These values
verify ordinary no-setup Starts against Google's published quota documentation.
"""
LIMITS = {
    "gmail_project_units_per_minute": 1_200_000,
    "gmail_account_units_per_minute": 6_000,
    "calendar_project_units_per_minute": 10_000,
    "calendar_account_units_per_minute": 600,
}
