import secrets


def create_journey_people(prefix, numbers=range(1, 10)):
    return {
        number: [
            {"username": f"{prefix}-j{number}-p{index}", "password": secrets.token_urlsafe(32)}
            for index in range(3)
        ]
        for number in numbers
    }
