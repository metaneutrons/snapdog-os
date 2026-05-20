################################################################################
#
# snapdog-users
#
################################################################################

define SNAPDOG_USERS_USERS
    audio 2001 audio 2001 * - - - Audio hardware
endef

$(eval $(generic-package))
