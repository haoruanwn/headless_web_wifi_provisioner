# Headless Wi-Fi Provisioner API

This document describes the HTTP JSON API provided by the `provisioner-daemon` for communication between the frontend UI and the backend.

## Endpoints

### 1. Scan for Wi-Fi Networks

- **URL**: `/api/scan`
- **Method**: `GET`
- **Description**: Triggers the backend to scan for available Wi-Fi networks and returns a list of found networks.

- **Success Response (200 OK)**:
  - **Content-Type**: `application/json`
  - **Body**: A JSON array of network objects.

    ```json
    [
      {
        "ssid": "MyHomeWiFi",
        "signal": 95,
        "security": "WPA3"
      },
      {
        "ssid": "CafeGuest",
        "signal": 78,
        "security": "Open"
      }
    ]
    ```

  - **Network Object Fields**:
    - `ssid` (string): The broadcast name of the network.
    - `signal` (number): The signal strength as a percentage (0-100).
    - `security` (string): The security protocol used (e.g., "WPA2", "WPA3", "WEP", "Open").

- **Error Response (500 Internal Server Error)**:
  ```json
  {
    "error": "A description of what went wrong."
  }
  ```

### 2. Connect to a Wi-Fi Network

- **URL**: `/api/connect`
- **Method**: `POST`
- **Description**: Attempts to connect to a specified Wi-Fi network using the provided credentials.

- **Request Body**:
  - **Content-Type**: `application/json`

    ```json
    {
      "ssid": "MyHomeWiFi",
      "password": "mySecretPassword123"
    }
    ```

- **Success Response (200 OK)**:
  ```json
  {
    "status": "success"
  }
  ```

- **Error Response (500 Internal Server Error)**:
  ```json
  {
    "error": "A description of the connection failure (e.g., invalid password)."
  }
  ```
