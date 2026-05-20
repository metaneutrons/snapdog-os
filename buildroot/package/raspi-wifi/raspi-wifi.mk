################################################################################
#
# raspi-wifi
#
# Pure dependency package — pulls in hostapd, wpa_supplicant, and firmware.
# Network management (AP mode, WiFi connect, DHCP) is handled by snapdog-ctrl.
#
################################################################################

$(eval $(generic-package))
