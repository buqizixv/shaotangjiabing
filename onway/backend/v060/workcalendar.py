"""China 2026 working dates, including official compensating workdays.

Source: https://www.beijing.gov.cn/zhengce/zhengcefagui/202511/t20251104_4258873.html
Unknown years deliberately require a calendar update rather than guessing.
"""
from datetime import date, timedelta

RANGES = [("01-01", "01-03"), ("02-15", "02-23"), ("04-04", "04-06"),
          ("05-01", "05-05"), ("06-19", "06-21"), ("09-25", "09-27"), ("10-01", "10-07")]
WORKDAYS = {"01-04", "02-14", "02-28", "05-09", "09-20", "10-10"}

def is_workday(day):
    if day.year != 2026:
        return None
    value = day.strftime("%m-%d")
    if value in WORKDAYS:
        return True
    if any(start <= value <= end for start, end in RANGES):
        return False
    return day.weekday() < 5
